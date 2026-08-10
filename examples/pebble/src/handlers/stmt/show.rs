//! `show expr;`
use nh_runtime::{Ctx, Result};
use crate::{Interp, Value};

pub fn run(host: &mut Interp, value: Value, _cx: &mut Ctx) -> Result<Value> {
    // Collected rather than printed, so `main` can still show what a
    // partially-recovered run produced.
    host.output.push(value.to_string());
    Ok(Value::Null)
}
