//! Handler for `stmt_continue`.
//!
//! See `stmt_break.rs`. The only difference is which of the enclosing loop's
//! two targets this jump is patched to — the end, or the step.

use nh_runtime::{Ctx, Result};

use crate::Interp;

pub fn run(host: &mut Interp, cx: &mut Ctx) -> Result<()> {
    let jump = host.emit_jump();
    match host.continue_to(jump) {
        true => Ok(()),
        false => cx.err("`continue` is not inside a loop"),
    }
}
