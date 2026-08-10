//! Handler for `stmt_iff` — `if cond then body`.
//!
//! The same handler shape as the interpreter's, read differently.
//!
//! An interpreter's version says "run the body if the condition holds". This
//! one says "emit the body *here*, and emit a jump around it". `lazy` is what
//! makes both possible: without it the body would already have been emitted,
//! before this handler could put a jump in front of it.
//!
//! Note the body is emitted **once**, even though it may run many times — the
//! opposite of the interpreter, where `.eval()` is called once per execution.

use nh_runtime::{Ctx, Result, Shared};

use crate::generated::ast::Stmt;
use crate::generated::dispatch::Eval;
use crate::Interp;

pub fn run(host: &mut Interp, _cond: (), body: &Shared<Stmt>, cx: &mut Ctx) -> Result<()> {
    // `cond` is already on the stack: its code was emitted before this ran.
    let jump = host.emit_jump_if_false();
    body.eval(host, cx)?;          // emits the body at this point in the stream
    host.patch_to_here(jump);      // now that its length is known
    Ok(())
}
