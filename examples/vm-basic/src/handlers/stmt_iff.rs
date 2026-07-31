//! `"if" "(" cond:expr ")" lazy body:block -> iff`
//!
//! `cond` is already in a register. `body` is **not** emitted yet — that is
//! what `lazy` buys, and without it the jump could not go in front of the body.
use nh_runtime::{Ctx, Result, Shared};
use nh_vm::{Op, Reg};

use crate::generated::ast::Block;
use crate::generated::dispatch::Eval;
use crate::Interp;

pub fn run(host: &mut Interp, cond: Reg, body: &Shared<Block>, cx: &mut Ctx) -> Result<Reg> {
    let skip = host.emit(Op::JumpIfFalse { src: cond, target: usize::MAX });
    body.eval(host, cx)?;       // emitted here, after the jump
    host.patch_to_here(skip);   // now that its length is known
    host.reset_regs();
    Ok(cond)
}
