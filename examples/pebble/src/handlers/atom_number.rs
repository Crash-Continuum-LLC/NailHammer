//! `NUMBER` → a number.
use nh_runtime::{Ctx, Result};
use crate::{Interp, Value};

pub fn run(_host: &mut Interp, value: &str, cx: &mut Ctx) -> Result<Value> {
    match value.parse() {
        Ok(n) => Ok(Value::Num(n)),
        Err(_) => cx.err(format!("`{value}` is not a number Pebble can hold")),
    }
}
