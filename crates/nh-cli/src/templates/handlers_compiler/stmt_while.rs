//! Handler for `stmt_while`.
//!
//! The same handler shape as the interpreter's, read differently. An
//! interpreter runs the body until the test fails; this emits the body **once**
//! and arranges jumps so the machine runs it many times.
//!
//! ```text
//! while c { b }
//!
//!   top: <c> · JumpIfFalse end · <b> · Jump top · end:
//! ```
//!
//! `lazy` on `cond` matters as much as on `body`: the condition's code has to
//! be emitted *inside* the loop, at `top`, so it is re-executed each time
//! round. An eager binding would have emitted it once, before the loop.

use std::rc::Rc;

use nh_runtime::{Ctx, Result};

use crate::generated::ast::{Block, Expr};
use crate::generated::dispatch::Eval;
use crate::Interp;

pub fn run(host: &mut Interp, cond: &Rc<Expr>, body: &Rc<Block>, cx: &mut Ctx) -> Result<()> {
    let top = host.here();
    cond.eval(host, cx)?;
    let to_end = host.emit_jump_if_false();

    // `break` and `continue` inside the body will find this frame and record
    // their jumps on it. An interpreter raises a signal and unwinds; a compiler
    // has nothing to unwind, so it writes down where to patch.
    host.enter_loop(top);
    body.eval(host, cx)?;
    host.emit_jump_to(top);

    host.patch_to_here(to_end);
    host.exit_loop(top);
    Ok(())
}
