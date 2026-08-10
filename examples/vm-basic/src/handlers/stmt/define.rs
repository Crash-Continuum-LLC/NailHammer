//! `"fn" name:IDENT "(" (params:IDENT ","?)* ")" lazy body:block -> define`
//!
//! A definition is emitted **inline, behind a jump**. The body has to live
//! somewhere in the instruction stream, and putting it here — rather than
//! collecting definitions and appending them — means a function is compiled
//! where it was written, so its address is known as soon as it exists.
//!
//! `lazy` is what makes that possible: without it the body would already have
//! been emitted, in front of the jump that is supposed to skip it.
use nh_runtime::{Ctx, Result, Shared};
use nh_vm::{Emitter, Op, Reg};

use crate::generated::ast::Block;
use crate::generated::dispatch::Eval;
use crate::Interp;

pub fn run(
    host: &mut Interp,
    name: &str,
    params: &[String],
    body: &Shared<Block>,
    cx: &mut Ctx,
) -> Result<Reg> {
    let over = host.emit(Op::Jump(usize::MAX));

    let addr = host.begin_fn(params);
    body.eval(host, cx)?;
    host.end_fn(name, addr, params.len());

    host.patch_to_here(over);
    host.reset_regs();
    Ok(0)
}
