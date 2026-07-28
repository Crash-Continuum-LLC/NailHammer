//! Tagged access to a parse tree.
//!
//! This module exists so that the §2 caveat in DESIGN.md is fixed in **one
//! place** rather than re-derived by every generated accessor.
//!
//! Pest's `Pairs::find_first_tagged` is built on `.flatten()` and searches the
//! entire subtree. On a recursive rule like `expr`, an outer node looking up
//! `lhs` will happily find an *inner* node's `lhs`, silently. Every lookup here
//! scans direct children only.

use pest::iterators::Pair;
use pest::RuleType;

use crate::source::{FileId, Span};

/// Finds the direct child carrying `tag`.
///
/// Deliberately not `find_first_tagged`.
pub fn tagged<'i, R: RuleType>(pair: &Pair<'i, R>, tag: &str) -> Option<Pair<'i, R>> {
    pair.clone()
        .into_inner()
        .find(|p| p.as_node_tag() == Some(tag))
}

/// All direct children carrying `tag`, in source order.
pub fn tagged_all<'i, R: RuleType>(pair: &Pair<'i, R>, tag: &str) -> Vec<Pair<'i, R>> {
    pair.clone()
        .into_inner()
        .filter(|p| p.as_node_tag() == Some(tag))
        .collect()
}

/// A bound node in the parse tree.
///
/// Generated accessors hand these back instead of raw `Pair`s, so handler code
/// never calls `into_inner()` and never indexes by position.
#[derive(Clone, Debug)]
pub struct Node<'i, R: RuleType> {
    pair: Pair<'i, R>,
    file: FileId,
}

impl<'i, R: RuleType> Node<'i, R> {
    pub fn new(pair: Pair<'i, R>, file: FileId) -> Self {
        Node { pair, file }
    }

    /// The matched source text, exactly as written.
    pub fn text(&self) -> &'i str {
        self.pair.as_str()
    }

    pub fn span(&self) -> Span {
        let s = self.pair.as_span();
        Span::new(self.file, s.start() as u32, s.end() as u32)
    }

    pub fn rule(&self) -> R {
        self.pair.as_rule()
    }

    pub fn file(&self) -> FileId {
        self.file
    }

    /// The underlying pair, for the cases generated code does not cover.
    pub fn pair(&self) -> &Pair<'i, R> {
        &self.pair
    }

    pub fn into_pair(self) -> Pair<'i, R> {
        self.pair
    }

    /// Wraps a direct child by tag. Used by generated views.
    pub fn tagged(&self, tag: &str) -> Option<Node<'i, R>> {
        tagged(&self.pair, tag).map(|p| Node::new(p, self.file))
    }

    pub fn tagged_all(&self, tag: &str) -> Vec<Node<'i, R>> {
        tagged_all(&self.pair, tag)
            .into_iter()
            .map(|p| Node::new(p, self.file))
            .collect()
    }

    /// Direct children, untagged included.
    pub fn children(&self) -> impl Iterator<Item = Node<'i, R>> + use<'i, R> {
        let file = self.file;
        self.pair.clone().into_inner().map(move |p| Node::new(p, file))
    }
}

/// A bound node whose token folds case.
///
/// Generated only for bindings that resolve to a `case-insensitive` token, so
/// calling [`Ident::key`] on a case-sensitive grammar is a compile error rather
/// than a silent identity function — and forgetting to fold at a symbol-table
/// lookup cannot silently miss.
#[derive(Clone, Debug)]
pub struct Ident<'i, R: RuleType> {
    node: Node<'i, R>,
}

impl<'i, R: RuleType> Ident<'i, R> {
    pub fn new(node: Node<'i, R>) -> Self {
        Ident { node }
    }

    /// The spelling as written. Use this in diagnostics: reporting `counter`
    /// when the programmer typed `COUNTER` reads as a compiler bug.
    pub fn text(&self) -> &'i str {
        self.node.text()
    }

    /// The folded canonical form, for symbol-table lookup.
    ///
    /// ASCII-only, matching the folding pest performs when matching
    /// (DESIGN.md §5.3).
    pub fn key(&self) -> String {
        self.node.text().to_ascii_lowercase()
    }

    pub fn span(&self) -> Span {
        self.node.span()
    }

    pub fn node(&self) -> &Node<'i, R> {
        &self.node
    }
}

/// Behaviour common to every generated view.
///
/// These live on a trait rather than as inherent methods for a specific reason:
/// a grammar may bind a field named `text`, `span`, or `node`, and the
/// generated accessor for it is an *inherent* method. Rust resolves inherent
/// methods before trait methods, so the user's binding wins and the built-in
/// stays reachable as `View::text(&view)`. Making these inherent would make
/// such a grammar fail to compile for no reason the author could see.
pub trait View<'i, R: RuleType>: Sized {
    fn from_pair(pair: Pair<'i, R>, file: FileId) -> Self;

    /// The whole matched region of this node.
    fn node(&self) -> &Node<'i, R>;

    fn span(&self) -> Span {
        self.node().span()
    }

    /// The matched source text, exactly as written.
    fn text(&self) -> &'i str {
        self.node().text()
    }
}

#[cfg(test)]
mod tests {
    // Behavioural coverage lives in nh-codegen's tests, which run generated
    // accessors against a real parse tree. Testing `tagged` here would require
    // a grammar, and the property that matters — that a nested tag does not
    // leak to an enclosing node — is only observable through one.
}
