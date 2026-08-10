//! Handler for `value_yes`.

use nh_runtime::{Ctx, Result};

use crate::{Interp, Value};

pub fn run(_host: &mut Interp, _cx: &mut Ctx) -> Result<Value> {
    Ok(Value::Bool(true))
}
