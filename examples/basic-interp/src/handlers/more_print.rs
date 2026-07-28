//! Handler for `more_print` — one `, expr` after the first `PRINT` argument.
//!
//! From `rule more_print = "," value:expr -> next;`

use nh_runtime::{Ctx, Result};

use crate::{Interp, Value};

pub fn run(_host: &mut Interp, value: Value, _cx: &mut Ctx) -> Result<Value> {
    Ok(value)
}
