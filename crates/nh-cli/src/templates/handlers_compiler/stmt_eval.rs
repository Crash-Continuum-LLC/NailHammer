//! Handler for `stmt_eval`.
//!
//! From `| value:expr ";" -> eval`
//!
//! The value is emitted but unused, so drop it — otherwise the stack grows by
//! one for every bare expression statement.

use nh_runtime::{Ctx, Result};

use crate::Interp;

pub fn run(host: &mut Interp, _value: (), _cx: &mut Ctx) -> Result<()> {
    host.emit_pop();
    Ok(())
}
