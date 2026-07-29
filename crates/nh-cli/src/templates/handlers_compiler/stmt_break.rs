//! Handler for `stmt_break`.
//!
//! This is where the two shapes genuinely part company, and neither is doing
//! the other's job badly.
//!
//! An interpreter raises `Error::Signal`, because at run time there *is* a
//! stack to unwind and `?` already unwinds it. A compiler has nothing to
//! unwind — the loop it is leaving has not run yet, and will not until long
//! after this program has exited. So it emits a jump whose target is not known
//! and hands the index to the enclosing loop, which fills it in once it knows
//! where its own end is.
//!
//! Nothing in the generated code forces either choice.

use nh_runtime::{Ctx, Result};

use crate::Interp;

pub fn run(host: &mut Interp, cx: &mut Ctx) -> Result<()> {
    let jump = host.emit_jump();
    match host.break_to(jump) {
        true => Ok(()),
        // The interpreter would have reported this at run time, when the signal
        // reached the top uncaught. A compiler can say it now.
        false => cx.err("`break` is not inside a loop"),
    }
}
