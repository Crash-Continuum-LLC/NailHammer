//! Handler for `stmt_iff` — `if cond { .. } else { .. }`.
//!
//! `then` and `otherwise` are `lazy`, which is the whole point: a branch must
//! not run before it is chosen. Every other binding arrives already evaluated,
//! and `cond` does too — it is evaluated exactly once, which is correct here
//! and would be wrong in a loop.

use std::rc::Rc;

use nh_runtime::{Ctx, Result};

use crate::generated::ast::{Block, ElseTail};
use crate::generated::dispatch::{Eval, Values};
use crate::{Interp, Value};

pub fn run(
    host: &mut Interp,
    cond: Value,
    then: &Rc<Block>,
    otherwise: Option<&Rc<ElseTail>>,
    cx: &mut Ctx,
) -> Result<Value> {
    // The same `truthy` that `&&` and `||` use. Asking here rather than
    // matching on `Value::Bool` is what keeps one notion of truth in the
    // language instead of two.
    if host.truthy(&cond) {
        then.eval(host, cx)
    } else if let Some(tail) = otherwise {
        tail.eval(host, cx)
    } else {
        Ok(Value::Unit)
    }
}
