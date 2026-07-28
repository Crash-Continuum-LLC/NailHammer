//! Handler for `entry`.
//!
//! From `rule entry = key:IDENT "=" value:value ";" -> entry;`

use nh_runtime::{Ctx, Result};

use crate::{Interp, Value};

/// The parameters are the bindings, in grammar order. Reorder the rule and
/// nothing here changes; rename a binding and this stops compiling.
pub fn run(_host: &mut Interp, key: &str, value: Value, _cx: &mut Ctx) -> Result<Value> {
    Ok(Value::Field(key.to_string(), Box::new(value)))
}
