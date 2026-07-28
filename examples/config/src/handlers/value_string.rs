//! Handler for `value_string`.

use nh_runtime::{Ctx, Result};

use crate::{Interp, Value};

pub fn run(_host: &mut Interp, raw: &str, _cx: &mut Ctx) -> Result<Value> {
    // The token includes its quotes.
    Ok(Value::Str(raw[1..raw.len() - 1].to_string()))
}
