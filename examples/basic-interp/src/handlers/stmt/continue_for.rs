//! Handler for `stmt_continue_for`.
//!
//! A non-local jump: the frame that has to move is the enclosing FOR, which
//! is several levels up. `Error::Signal` is the channel, and the label names
//! the construct — so a different kind of loop in between ignores it and lets
//! it through.
//!
//! The label is spelled the way the *language* spells it, because an uncaught
//! signal reports against that name — `` `EXIT SUB` is not inside anything that
//! handles it `` reads like the program, where `exit-sub` would leak a spelling
//! the programmer never wrote.

use nh_runtime::{Ctx, Result};

use crate::{Interp, Value};

pub fn run(_host: &mut Interp, cx: &mut Ctx) -> Result<Value> {
    Err(cx.signal("CONTINUE FOR"))
}
