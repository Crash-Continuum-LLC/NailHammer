//! Handler for `stmt_iff` — `if cond then body`.
//!
//! From `| "if" cond:expr "then" lazy body:stmt -> iff`
//!
//! `body` is the one parameter that does **not** arrive evaluated, because the
//! grammar marked it `lazy`. It is owned data, so this handler could keep it as
//! easily as run it; calling `.eval(..)` is what makes it happen — and an `if`
//! whose condition is false never does.

use nh_runtime::{Ctx, Result, Shared};

use crate::generated::ast::Stmt;
use crate::generated::dispatch::{Eval, Values};
use crate::{Interp, Value};

pub fn run(host: &mut Interp, cond: Value, body: &Shared<Stmt>, cx: &mut Ctx) -> Result<Value> {
    // `truthy` and not a match on `Value::Bool(false)`: the language has one
    // definition of truth, and it lives in `Semantics`. The short-circuit
    // defaults for `&&` and `||` already use it, so writing the test out by
    // hand here would let `if 0 then ..` and `0 && ..` disagree — and this
    // interpreter does count `0` as false.
    if !host.truthy(&cond) {
        return Ok(cond);
    }
    body.eval(host, cx)
}
