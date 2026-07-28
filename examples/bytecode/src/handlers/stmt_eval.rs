//! Handler for `stmt_eval`.
//!
//! From `| value:expr ";" -> eval`

use nh_runtime::{Ctx, Result};

use crate::Interp;

pub fn run(host: &mut Interp, _value: (), _cx: &mut Ctx) -> Result<()> {
    host.emit_pop();
    Ok(())
}
