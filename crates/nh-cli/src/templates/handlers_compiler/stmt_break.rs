//! A compiler has nothing to unwind — the loop has not run yet. It emits a
//! jump with no target and hands the index to the enclosing loop.
use nh_runtime::{Ctx, Result};
use crate::{Interp, Reg};

pub fn run(host: &mut Interp, cx: &mut Ctx) -> Result<Reg> {
    let jump = host.emit_jump();
    if host.break_to(jump) {
        Ok(host.next_reg())
    } else {
        cx.err("`break` is not inside a loop")
    }
}
