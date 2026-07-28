//! Handler for `program`.
//!
//! From `rule program = SOI stmts:stmt* EOI -> doc;`

use nh_runtime::{Ctx, Result};

use crate::{Interp, Value};

pub fn run(_host: &mut Interp, stmts: Vec<Value>, _cx: &mut Ctx) -> Result<Value> {
    // `stmts` is a Vec because the grammar says `stmt*`, and each one is
    // already evaluated. Change the `*` to a `?` and this stops compiling —
    // cardinality lives in the type.
    //
    // A statement the parser recovered from was reported by `syntax_errors`
    // and is simply not in this Vec, so every statement that *can* run has
    // already run. That is what makes recovery worth having.
    Ok(stmts.into_iter().last().unwrap_or(Value::Bool(true)))
}
