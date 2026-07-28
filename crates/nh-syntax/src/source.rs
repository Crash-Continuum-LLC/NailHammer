//! Multi-file source tracking.
//!
//! DESIGN.md §7 chose a full `SourceMap` over single-file spans up front,
//! because retrofitting a `FileId` through every span, diagnostic, and view
//! signature later is far more expensive than carrying it from the start. M0
//! already needs it: `import` (§3.1) means a duplicate-definition error must
//! point at two locations in two different files.

use std::fmt;
use std::path::{Path, PathBuf};

/// Index of a file within a [`SourceMap`].
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct FileId(pub u32);

/// A byte range within a specific file.
///
/// Offsets are `u32`: a single source file above 4 GiB is not a case worth
/// doubling every span for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Span {
    pub file: FileId,
    pub lo: u32,
    pub hi: u32,
}

impl Span {
    pub fn new(file: FileId, lo: u32, hi: u32) -> Self {
        Span { file, lo, hi }
    }

    /// A zero-width span at the start of `file`, for diagnostics that concern a
    /// file as a whole rather than any position in it.
    pub fn whole_file(file: FileId) -> Self {
        Span { file, lo: 0, hi: 0 }
    }

    pub fn join(self, other: Span) -> Span {
        debug_assert_eq!(self.file, other.file, "cannot join spans across files");
        Span {
            file: self.file,
            lo: self.lo.min(other.lo),
            hi: self.hi.max(other.hi),
        }
    }
}

/// A value paired with the source location it came from.
#[derive(Clone, PartialEq, Eq)]
pub struct Spanned<T> {
    pub value: T,
    pub span: Span,
}

impl<T> Spanned<T> {
    pub fn new(value: T, span: Span) -> Self {
        Spanned { value, span }
    }
}

impl<T: fmt::Debug> fmt::Debug for Spanned<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.value.fmt(f)
    }
}

impl<T> std::ops::Deref for Spanned<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}

/// One-based line and column, for display.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LineCol {
    pub line: u32,
    pub col: u32,
}

struct SourceFile {
    path: PathBuf,
    text: String,
    /// Byte offset of the start of each line.
    line_starts: Vec<u32>,
}

impl SourceFile {
    fn new(path: PathBuf, text: String) -> Self {
        let mut line_starts = vec![0u32];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i as u32 + 1);
            }
        }
        SourceFile { path, text, line_starts }
    }

    fn line_col(&self, offset: u32) -> LineCol {
        // Index of the last line start <= offset.
        let line = match self.line_starts.binary_search(&offset) {
            Ok(i) => i,
            Err(i) => i - 1,
        };
        let line_start = self.line_starts[line] as usize;
        let offset = (offset as usize).min(self.text.len());
        // Count characters, not bytes, so columns are meaningful in UTF-8 text.
        let col = self.text[line_start..offset].chars().count();
        LineCol {
            line: line as u32 + 1,
            col: col as u32 + 1,
        }
    }

    fn line_text(&self, line: u32) -> &str {
        let idx = (line - 1) as usize;
        let start = self.line_starts[idx] as usize;
        let end = self
            .line_starts
            .get(idx + 1)
            .map(|&e| e as usize)
            .unwrap_or(self.text.len());
        self.text[start..end].trim_end_matches(['\n', '\r'])
    }
}

/// Interned set of source files.
#[derive(Default)]
pub struct SourceMap {
    files: Vec<SourceFile>,
}

impl SourceMap {
    pub fn new() -> Self {
        SourceMap::default()
    }

    pub fn add(&mut self, path: impl Into<PathBuf>, text: impl Into<String>) -> FileId {
        let id = FileId(self.files.len() as u32);
        self.files.push(SourceFile::new(path.into(), text.into()));
        id
    }

    /// Returns the id of an already-added file with this path, if any.
    ///
    /// Import resolution uses this for diamond dedup (§3.1): the same file
    /// reached by two import paths is loaded once, and is not a duplicate
    /// definition.
    pub fn find_by_path(&self, path: &Path) -> Option<FileId> {
        self.files
            .iter()
            .position(|f| f.path == path)
            .map(|i| FileId(i as u32))
    }

    pub fn path(&self, file: FileId) -> &Path {
        &self.files[file.0 as usize].path
    }

    pub fn text(&self, file: FileId) -> &str {
        &self.files[file.0 as usize].text
    }

    pub fn snippet(&self, span: Span) -> &str {
        let text = self.text(span.file);
        let lo = (span.lo as usize).min(text.len());
        let hi = (span.hi as usize).min(text.len());
        &text[lo..hi]
    }

    pub fn line_col(&self, span: Span) -> LineCol {
        self.files[span.file.0 as usize].line_col(span.lo)
    }

    pub fn line_text(&self, span: Span) -> &str {
        let file = &self.files[span.file.0 as usize];
        file.line_text(file.line_col(span.lo).line)
    }

    /// `path:line:col`, the clickable form.
    pub fn location(&self, span: Span) -> String {
        let lc = self.line_col(span);
        format!("{}:{}:{}", self.path(span.file).display(), lc.line, lc.col)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_col_is_one_based() {
        let mut sm = SourceMap::new();
        let f = sm.add("a.nh", "grammar A;\nrule x = y;\n");
        assert_eq!(sm.line_col(Span::new(f, 0, 1)), LineCol { line: 1, col: 1 });
        assert_eq!(sm.line_col(Span::new(f, 11, 15)), LineCol { line: 2, col: 1 });
        assert_eq!(sm.line_col(Span::new(f, 16, 17)), LineCol { line: 2, col: 6 });
    }

    #[test]
    fn columns_count_chars_not_bytes() {
        let mut sm = SourceMap::new();
        // 'ü' is two bytes; the column after it must be 2, not 3.
        let f = sm.add("a.nh", "ü=1");
        assert_eq!(sm.line_col(Span::new(f, 2, 3)).col, 2);
    }

    #[test]
    fn line_text_strips_the_newline() {
        let mut sm = SourceMap::new();
        let f = sm.add("a.nh", "one\ntwo\r\nthree");
        assert_eq!(sm.line_text(Span::new(f, 4, 5)), "two");
        assert_eq!(sm.line_text(Span::new(f, 9, 10)), "three");
    }
}
