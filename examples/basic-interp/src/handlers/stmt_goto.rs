//! Handler for `stmt_goto` — `GOTO 100`.
//!
//! From `| "GOTO" target:NUMBER -> goto`
//!
//! A jump is not something a handler can do by returning: the frame that has to
//! move is `program`, several levels up. `Error::Signal` is the channel — `?`
//! propagation is already exactly the unwinding a jump needs, and the signal is
//! the variant that says "this is not a failure".
//!
//! The **target** rides on the interpreter rather than in the signal, because
//! the runtime has no idea what a BASIC line number is.

use nh_runtime::{Ctx, Result};

use crate::{Interp, Value};

pub fn run(host: &mut Interp, target: &str, cx: &mut Ctx) -> Result<Value> {
    host.jump = Some(target.to_string());
    Err(cx.signal("goto"))
}
