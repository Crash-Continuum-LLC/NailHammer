//! Handler for `stmt_blank` — a bare `PRINT`, which prints an empty line.
//!
//! From `| "PRINT" -> blank`

use nh_runtime::{Ctx, Result};

use crate::{Interp, Value};

pub fn run(host: &mut Interp, _cx: &mut Ctx) -> Result<Value> {
    host.output.push(String::new());
    Ok(Value::Nothing)
}
