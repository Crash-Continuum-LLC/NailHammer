//! Handler for `more_arg` — one `, expr` after the first argument.
//!
//! From `rule more_arg = "," value:expr -> next;`

use nh_runtime::{Ctx, Result};

use crate::{Interp, Value};

pub fn run(_host: &mut Interp, value: Value, _cx: &mut Ctx) -> Result<Value> {
    Ok(value)
}
