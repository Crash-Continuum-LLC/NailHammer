//! Source tracking for *target* programs.
//!
//! Deliberately separate from `nh-syntax`'s `SourceMap`, which tracks `.nh`
//! grammar files. They serve different processes — compiling a grammar versus
//! running a program written in the resulting language — and a user's parser
//! should not have to depend on NailHammer's own grammar parser to report a
//! type error.
//!
//! Multi-file from the start (DESIGN.md §7): target languages want
//! `import`/`include`, and threading a `FileId` through every span, diagnostic,
//! and view signature after the fact is far more expensive than carrying it.

use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct FileId(pub u32);

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

    pub fn join(self, other: Span) -> Span {
        debug_assert_eq!(self.file, other.file, "cannot join spans across files");
        Span {
            file: self.file,
            lo: self.lo.min(other.lo),
            hi: self.hi.max(other.hi),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LineCol {
    pub line: u32,
    pub col: u32,
}

struct SourceFile {
    path: PathBuf,
    text: String,
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
        SourceFile {
            path,
            text,
            line_starts,
        }
    }

    fn line_col(&self, offset: u32) -> LineCol {
        let line = match self.line_starts.binary_search(&offset) {
            Ok(i) => i,
            Err(i) => i - 1,
        };
        let start = self.line_starts[line] as usize;
        let offset = (offset as usize).min(self.text.len());
        // Characters, not bytes, so columns mean something in UTF-8 source.
        let col = self.text[start..offset].chars().count();
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

    /// Loads a file from disk.
    pub fn load(&mut self, path: impl AsRef<Path>) -> std::io::Result<FileId> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)?;
        Ok(self.add(path.to_path_buf(), text))
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
        let f = &self.files[span.file.0 as usize];
        f.line_text(f.line_col(span.lo).line)
    }

    pub fn location(&self, span: Span) -> String {
        let lc = self.line_col(span);
        format!("{}:{}:{}", self.path(span.file).display(), lc.line, lc.col)
    }
}

impl fmt::Debug for SourceMap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SourceMap")
            .field("files", &self.files.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_col_is_one_based_and_counts_chars() {
        let mut sm = SourceMap::new();
        let f = sm.add("a.txt", "let x = 1;\nlet ü = 2;\n");
        assert_eq!(sm.line_col(Span::new(f, 0, 1)), LineCol { line: 1, col: 1 });
        assert_eq!(sm.line_col(Span::new(f, 11, 14)), LineCol { line: 2, col: 1 });
        // 'ü' is two bytes; the column after it must be 6, not 7.
        assert_eq!(sm.line_col(Span::new(f, 17, 18)).col, 6);
    }

    #[test]
    fn line_text_strips_the_newline() {
        let mut sm = SourceMap::new();
        let f = sm.add("a.txt", "one\ntwo\r\nthree");
        assert_eq!(sm.line_text(Span::new(f, 4, 5)), "two");
    }
}
