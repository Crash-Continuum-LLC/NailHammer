//! Handler for `value_number`.

use nh_runtime::{Ctx, Result};

use crate::{Interp, Value};

pub fn run(_host: &mut Interp, digits: &str, cx: &mut Ctx) -> Result<Value> {
    match digits.parse::<f64>() {
        Ok(n) => Ok(Value::Num(n)),
        // No span threading: `cx.err` picks up this node's location because
        // dispatch pushed it before calling us.
        Err(_) => cx.err("not a valid number"),
    }
}
