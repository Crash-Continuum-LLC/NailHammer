//! Handler for `value_list`.
//!
//! From `| "[" items:value* "]" -> list`

use nh_runtime::{Ctx, Result};

use crate::{Interp, Value};

pub fn run(_host: &mut Interp, items: Vec<Value>, _cx: &mut Ctx) -> Result<Value> {
    Ok(Value::List(items))
}
