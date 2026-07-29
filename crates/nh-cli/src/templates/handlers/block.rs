//! Handler for `block` — a group of statements.
//!
//! Every construct that owns a body takes one of these, so `if` and the loops
//! need to know nothing about what is inside one.

use nh_runtime::{Ctx, Result};

use crate::{Interp, Value};

pub fn run(_host: &mut Interp, stmts: Vec<Value>, _cx: &mut Ctx) -> Result<Value> {
    // Already evaluated, in order. A block's value is its last statement's,
    // which is what makes `let x = { ... }` work if you ever add it.
    Ok(stmts.into_iter().last().unwrap_or(Value::Unit))
}
