//! `"while" "(" lazy cond:expr ")" lazy body:block -> whilst`
//!
//! **Both** operands are lazy, and the condition is the interesting one. A loop
//! re-tests every iteration, so its code has to sit at the top of the loop —
//! which means the handler has to know where the top *is* before the condition
//! is emitted. An eager `cond` would already be behind us.
use nh_runtime::{Ctx, Result, Shared};
use nh_vm::{Op, Reg};

use crate::generated::ast::{Block, Expr};
use crate::generated::dispatch::Eval;
use crate::Interp;

pub fn run(
    host: &mut Interp,
    cond: &Shared<Expr>,
    body: &Shared<Block>,
    cx: &mut Ctx,
) -> Result<Reg> {
    let top = host.here();
    let test = cond.eval(host, cx)?;
    let exit = host.emit(Op::JumpIfFalse { src: test, target: usize::MAX });
    body.eval(host, cx)?;
    host.emit(Op::Jump(top));
    host.patch_to_here(exit);
    host.reset_regs();
    Ok(test)
}
