//! Runtime support for NailHammer-generated parsers.
//!
//! This is the crate a *user's* project depends on. It deliberately does not
//! depend on `nh-syntax`: running a program written in your language should not
//! drag in the parser for `.nh` grammar files.
//!
//! What lives here is everything generated code needs but should not re-derive:
//!
//! * [`node`] — direct-child tag access, so DESIGN.md §2's `find_first_tagged`
//!   hazard is fixed once rather than in every generated accessor.
//! * [`ctx`] — the span stack that makes `cx.err(..)` locate itself.
//! * [`source`], [`diagnostic`] — multi-file spans and rustc-style rendering.
//! * [`error`] — including `AlreadyReported`, for cascade suppression.

pub mod ctx;
pub mod diagnostic;
pub mod error;
pub mod name;
pub mod node;
pub mod ops;
pub mod source;

pub use ctx::Ctx;
pub use diagnostic::{Diagnostic, Severity};
pub use error::{Error, Result};
pub use name::Name;
pub use node::{tagged, tagged_all, Ident, Node, View};
pub use ops::{Assoc, Fixity, OpInfo, OpTree};
pub use source::{FileId, LineCol, SourceMap, Span};

/// Re-exported so generated code has one place to reach for pest types and
/// users never have to match NailHammer's pest version by hand.
pub mod pest {
    pub use pest::iterators::{Pair, Pairs};
    pub use pest::{Parser, RuleType};
}
