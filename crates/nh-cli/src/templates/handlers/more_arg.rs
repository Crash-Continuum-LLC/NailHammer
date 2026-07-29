//! Handler for `more_arg` — one `, value` in an argument list.
//!
//! Arguments *are* evaluated, unlike parameters: this one arrives already
//! computed, in source order, and just passes through.

use nh_runtime::{Ctx, Result};

use crate::{Interp, Value};

pub fn run(_host: &mut Interp, value: Value, _cx: &mut Ctx) -> Result<Value> {
    Ok(value)
}
