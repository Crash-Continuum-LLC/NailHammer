//! Handler for `primary_num`.
//!
//! From `= value:NUMBER -> num`

use nh_runtime::{Ctx, Result};

use crate::{Interp, Value};

pub fn run(_host: &mut Interp, value: &str, cx: &mut Ctx) -> Result<Value> {
    match value.parse::<f64>() {
        Ok(n) => Ok(Value::Num(n)),
        Err(_) => cx.err("not a valid number"),
    }
}
