//! `let x = 1;`
use nh_runtime::{Ctx, Result};
use crate::{Interp, Value};

pub fn run(host: &mut Interp, name: &str, value: Value, _cx: &mut Ctx) -> Result<Value> {
    host.set(name, value.clone());
    Ok(value)
}
