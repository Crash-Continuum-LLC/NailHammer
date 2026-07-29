//! Handler for `stmt_return`.
//!
//! The same mechanism as `break`: a signal, carrying the fact that a jump
//! happened. The *value* does not ride on the signal — it goes on the host,
//! because the runtime has no idea what your values are and should not have to.
//! `primary_call` takes it back out.

use nh_runtime::{Ctx, Result};

use crate::{Interp, Value};

pub fn run(host: &mut Interp, value: Option<Value>, cx: &mut Ctx) -> Result<Value> {
    // `value:expr?` in the grammar, so a bare `return` is `None`.
    host.returning = Some(value.unwrap_or(Value::Unit));
    Err(cx.signal("return"))
}
