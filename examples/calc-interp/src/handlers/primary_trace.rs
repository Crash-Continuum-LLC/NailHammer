//! Handler for `primary_trace` — `trace(x)`.
//!
//! Records that it ran. This is what lets a test prove short-circuiting by
//! **observation**: if `false && trace(1)` ever evaluates its right operand,
//! the effect shows up in `host.traced`.

use nh_runtime::{Ctx, Result};

use crate::{Interp, Value};

pub fn run(host: &mut Interp, inner: Value, _cx: &mut Ctx) -> Result<Value> {
    if let Value::Num(n) = inner {
        host.traced.push(n);
    }
    Ok(inner)
}
