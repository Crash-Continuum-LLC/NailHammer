//! Handler for `stmt_eval` — a bare expression statement.

use nh_runtime::{Ctx, Result};

use crate::{Interp, Value};

pub fn run(host: &mut Interp, value: Value, _cx: &mut Ctx) -> Result<Value> {
    host.output.push(value.to_string());
    Ok(value)
}
