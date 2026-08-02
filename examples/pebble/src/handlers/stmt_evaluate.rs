//! A bare expression statement.
use nh_runtime::{Ctx, Result};
use crate::{Interp, Value};

pub fn run(_host: &mut Interp, value: Value, _cx: &mut Ctx) -> Result<Value> {
    Ok(value)
}
