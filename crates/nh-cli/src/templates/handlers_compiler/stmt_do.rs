//! Handler for `stmt_do` — a loop that runs its body before testing.
//!
//! ```text
//! do { b } while c;
//!
//!   top: <b> · test: <c> · JumpIfTrue top · end:
//! ```
//!
//! `continue` goes to `test`, not `top`: skipping the test would let the loop
//! run forever.

use std::rc::Rc;

use nh_runtime::{Ctx, Result};

use crate::generated::ast::{Block, Expr};
use crate::generated::dispatch::Eval;
use crate::Interp;

pub fn run(host: &mut Interp, body: &Rc<Block>, cond: &Rc<Expr>, cx: &mut Ctx) -> Result<()> {
    let top = host.here();

    host.enter_loop(top);
    body.eval(host, cx)?;

    let test = host.here();
    cond.eval(host, cx)?;
    let back = host.emit_jump_if_true();
    host.patch_to(back, top);

    host.exit_loop(test);
    Ok(())
}
