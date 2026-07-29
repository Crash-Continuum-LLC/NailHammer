//! Handler for `stmt_for` — a counting loop.
//!
//! `from` and `to` are *not* lazy, and that is correct: the bounds are fixed
//! when the loop starts, so `for i = 1 to n` does not notice `n` changing
//! inside the body. Only `body` re-runs, so only `body` is lazy.

use std::rc::Rc;

use nh_runtime::{Ctx, Error, Result};

use crate::generated::ast::Block;
use crate::generated::dispatch::Eval;
use crate::{Interp, Value};

pub fn run(
    host: &mut Interp,
    var: &str,
    from: Value,
    to: Value,
    body: &Rc<Block>,
    cx: &mut Ctx,
) -> Result<Value> {
    let (start, end) = match (&from, &to) {
        (Value::Num(a), Value::Num(b)) => (*a, *b),
        _ => return cx.err(format!("a loop counts over numbers, got {from} and {to}")),
    };

    let mut i = start;
    while i <= end {
        host.set(var, Value::Num(i));
        match body.eval(host, cx) {
            Ok(_) => {}
            Err(Error::Signal { label: "break", .. }) => break,
            Err(Error::Signal { label: "continue", .. }) => {}
            Err(other) => return Err(other),
        }
        i += 1.0;
    }
    Ok(Value::Unit)
}
