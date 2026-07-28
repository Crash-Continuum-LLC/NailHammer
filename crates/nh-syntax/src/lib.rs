//! Parser, AST, and import resolution for the NailHammer `.nh` intermediate
//! language.
//!
//! This crate is milestone **M0** of DESIGN.md: it takes `.nh` source and
//! produces a merged [`Ast`], with diagnostics that carry real file/line/column
//! locations. It performs no lowering to `.pest` and no semantic analysis —
//! those are M1 and M4.
//!
//! ```no_run
//! use nh_syntax::{resolve, SourceMap};
//! use std::path::Path;
//!
//! let mut sources = SourceMap::new();
//! match resolve(&mut sources, Path::new("example.nh")) {
//!     Ok(ast) => println!("{}", nh_syntax::render(&ast)),
//!     Err(errors) => eprint!("{}", errors.render(&sources)),
//! }
//! ```

pub mod ast;
pub mod error;
pub mod import;
pub mod parse;
pub mod render;
pub mod source;

pub use ast::Ast;
pub use error::{Diagnostic, Errors, Severity};
pub use import::resolve;
pub use parse::parse_file;
pub use render::{alternative_source, render};
pub use source::{FileId, SourceMap, Span, Spanned};

/// Parses `.nh` text that does not come from disk.
///
/// `name` is only used for diagnostics. This is what lets the operator presets
/// be stored as ordinary `.nh` source and parsed with the real parser, rather
/// than hand-built as Rust data structures — which is what makes DESIGN.md
/// §6.1's "presets are ordinary tables" true in the implementation and not just
/// in the prose.
pub fn parse_source(
    sm: &mut SourceMap,
    name: impl Into<std::path::PathBuf>,
    text: impl Into<String>,
) -> Result<Ast, Errors> {
    let id = sm.add(name, text);
    parse_file(sm, id)
}
