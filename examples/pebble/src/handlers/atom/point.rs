//! `(x, y)` — a point literal.
//!
//! Both coordinates arrive **evaluated**, so `(1 + 1, n * 2)` works without
//! this handler knowing that either was an expression.

use nh_runtime::{Ctx, Result};
use crate::{Interp, Value};

pub fn run(_host: &mut Interp, x: Value, y: Value, cx: &mut Ctx) -> Result<Value> {
    match (x, y) {
        (Value::Num(a), Value::Num(b)) => Ok(Value::Point(a, b)),
        (a, b) => cx.err(format!("a point needs two numbers, got `{a}` and `{b}`")),
    }
}
