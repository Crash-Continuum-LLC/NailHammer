//! `{ .. }` — the statements inside, already run.
use nh_runtime::{Ctx, Result};
use crate::{Interp, Value};

pub fn run(_host: &mut Interp, body: Vec<Value>, _cx: &mut Ctx) -> Result<Value> {
    Ok(body.into_iter().last().unwrap_or(Value::Null))
}
