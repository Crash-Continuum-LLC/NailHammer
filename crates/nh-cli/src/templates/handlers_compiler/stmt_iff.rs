//! Handler for `stmt_iff` — `if cond { .. } else { .. }`.
//!
//! The same handler shape as the interpreter's, read differently. An
//! interpreter says "run the chosen branch"; this says "emit both branches,
//! with jumps arranged so only one runs".
//!
//! `lazy` is what makes both possible. Without it the branches would already be
//! in the instruction stream before this could put a jump in front of them —
//! and both would run.
//!
//! ```text
//! if c { a } else { b }
//!
//!   <c> · JumpIfFalse else · <a> · Jump end · else: <b> · end:
//! ```

use std::rc::Rc;

use nh_runtime::{Ctx, Result};

use crate::generated::ast::{Block, ElseTail};
use crate::generated::dispatch::Eval;
use crate::Interp;

pub fn run(
    host: &mut Interp,
    _cond: (),
    then: &Rc<Block>,
    otherwise: Option<&Rc<ElseTail>>,
    cx: &mut Ctx,
) -> Result<()> {
    // `cond` is already on the stack: its code was emitted before this ran.
    let to_else = host.emit_jump_if_false();
    then.eval(host, cx)?;

    match otherwise {
        None => host.patch_to_here(to_else),
        Some(tail) => {
            // The `then` branch must not fall into the `else` branch.
            let to_end = host.emit_jump();
            host.patch_to_here(to_else);
            tail.eval(host, cx)?;
            host.patch_to_here(to_end);
        }
    }
    Ok(())
}
