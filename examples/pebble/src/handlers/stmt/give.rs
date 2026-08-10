//! `return;` or `return expr;`
//!
//! Leaves the value on the host and raises a **signal** — an `Err` that is not
//! a failure. `Interp::call` is the only thing that catches it, so a `return`
//! outside a function reaches the top as an error, which is what it is.

use nh_runtime::{Ctx, Result};
use crate::{Interp, Value};

pub fn run(host: &mut Interp, value: Option<Value>, cx: &mut Ctx) -> Result<Value> {
    host.stash_return(value.unwrap_or(Value::Null));
    Err(cx.signal("return"))
}
