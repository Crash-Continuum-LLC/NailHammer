//! An owned identifier, for the generated AST.
//!
//! [`crate::Ident`] borrows the parse tree, which is right while dispatch is
//! walking it and wrong for a node that outlives the walk. The AST is owned, so
//! its identifiers are too — and they keep *both* spellings, because losing
//! either is a bug the type can prevent: `.key()` to look a symbol up, `.text()`
//! to report it back the way it was written (DESIGN.md §5.3).

use crate::source::Span;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Name {
    text: String,
    key: String,
    span: Span,
}

impl Name {
    pub fn new(text: impl Into<String>, span: Span) -> Self {
        let text = text.into();
        // ASCII-only, matching the folding pest performs when matching.
        let key = text.to_ascii_lowercase();
        Name { text, key, span }
    }

    /// The spelling as written. Use this in diagnostics: reporting `counter`
    /// when the programmer typed `COUNTER` reads as a compiler bug.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The folded canonical form, for symbol-table lookup.
    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn span(&self) -> Span {
        self.span
    }
}

impl std::fmt::Display for Name {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::FileId;

    #[test]
    fn a_name_keeps_both_spellings() {
        let n = Name::new("COUNTER", Span::new(FileId(0), 0, 7));
        assert_eq!(n.text(), "COUNTER", "diagnostics get what was typed");
        assert_eq!(n.key(), "counter", "lookups get the folded form");
    }

    /// Folding is ASCII-only, matching pest's matcher — a Unicode pair that
    /// changes length under folding would not have matched in the first place.
    #[test]
    fn folding_leaves_non_ascii_alone() {
        let n = Name::new("Straße", Span::new(FileId(0), 0, 7));
        assert_eq!(n.key(), "straße");
    }
}
