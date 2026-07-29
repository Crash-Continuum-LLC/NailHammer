use nh_runtime::{Ctx, Result};
use crate::{Interp, Reg};

pub fn run(host: &mut Interp, cx: &mut Ctx) -> Result<Reg> {
    let jump = host.emit_jump();
    if host.continue_to(jump) {
        Ok(host.next_reg())
    } else {
        cx.err("`continue` is not inside a loop")
    }
}
