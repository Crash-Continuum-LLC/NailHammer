//! Handler for `step_clause` — the `STEP n` of a `FOR`.
//!
//! From `rule step_clause = "STEP" value:expr -> step;`

use nh_runtime::{Ctx, Result};

use crate::{Interp, Value};

pub fn run(_host: &mut Interp, value: Value, _cx: &mut Ctx) -> Result<Value> {
    Ok(value)
}
