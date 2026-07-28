//! Handler for `primary_num`.
//!
//! From `= digits:NUMBER -> num`

use nh_runtime::{Ctx, Result};

use crate::{Interp, Value};

pub fn run(_host: &mut Interp, digits: &str, cx: &mut Ctx) -> Result<Value> {
    match digits.parse::<f64>() {
        Ok(n) => Ok(Value::Num(n)),
        // No span threading: dispatch already pushed this node's location, so
        // `cx.err` knows where it is.
        Err(_) => cx.err("not a valid number"),
    }
}
