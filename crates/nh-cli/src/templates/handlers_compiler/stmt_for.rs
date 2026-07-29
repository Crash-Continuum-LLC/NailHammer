//! Handler for `stmt_for` — a counting loop.
//!
//! `from` and `to` are already on the stack: they are eager bindings, so their
//! code was emitted before this ran, which is right — the bounds are fixed when
//! the loop starts.
//!
//! ```text
//! for i = a to b { body }
//!
//!   <a> · Store i · Pop · <b> · Store limit · Pop
//!   top: Load i · Load limit · LtEq · JumpIfFalse end
//!        <body>
//!   next: Load i · Push 1 · Add · Store i · Pop · Jump top
//!   end:
//! ```
//!
//! `continue` lands on `next`, not `top` — skipping the increment would make
//! it an infinite loop. That is the whole reason a loop frame carries two
//! targets rather than one.

use std::rc::Rc;

use nh_runtime::{Ctx, Result};

use crate::generated::ast::Block;
use crate::generated::dispatch::Eval;
use crate::Interp;

pub fn run(
    host: &mut Interp,
    var: &str,
    _from: (),
    _to: (),
    body: &Rc<Block>,
    cx: &mut Ctx,
) -> Result<()> {
    // The bounds are on the stack, `to` on top. A hidden variable holds the
    // limit so the body cannot reach it.
    let limit = format!(" limit {var}");
    host.emit_store(&limit);
    host.emit_pop();
    host.emit_store(var);
    host.emit_pop();

    let top = host.here();
    host.emit_load(var);
    host.emit_load(&limit);
    host.emit_le();
    let to_end = host.emit_jump_if_false();

    host.enter_loop(top);
    body.eval(host, cx)?;

    // Where `continue` goes: the increment, then back to the test.
    let next = host.here();
    host.emit_load(var);
    host.emit_push(1.0);
    host.emit_add();
    host.emit_store(var);
    host.emit_pop();
    host.emit_jump_to(top);

    host.patch_to_here(to_end);
    host.exit_loop(next);
    Ok(())
}
