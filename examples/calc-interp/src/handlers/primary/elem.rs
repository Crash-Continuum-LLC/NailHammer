//! Handler for `primary_elem` — reading `a[i]`.

use nh_runtime::{Ctx, Result};

use crate::{Interp, Value};

pub fn run(host: &mut Interp, name: &str, index: Value, cx: &mut Ctx) -> Result<Value> {
    let i = match index {
        Value::Num(n) if n >= 0.0 && n.fract() == 0.0 => n as usize,
        other => return cx.err(format!("`{other}` is not a valid slot index")),
    };
    Ok(host
        .slots
        .get(name)
        .and_then(|s| s.get(i))
        .cloned()
        .unwrap_or(Value::Num(0.0)))
}
