//! `"if" "(" cond:expr ")" lazy body:block "else" lazy alt:block -> ifelse`
//!
//! Two blocks and two jumps:
//!
//! ```text
//!   <cond>
//!   JumpIfFalse -> otherwise
//!   <body>
//!   Jump        -> after
//! otherwise:
//!   <alt>
//! after:
//! ```
//!
//! Both blocks are `lazy` for the same reason the single-armed `if` needs one:
//! a jump has to be emitted in front of code that has not been emitted yet.
use nh_runtime::{Ctx, Result, Shared};
use nh_vm::{Emitter, Op, Reg};

use crate::generated::ast::Block;
use crate::generated::dispatch::Eval;
use crate::Interp;

pub fn run(
    host: &mut Interp,
    cond: Reg,
    body: &Shared<Block>,
    alt: &Shared<Block>,
    cx: &mut Ctx,
) -> Result<Reg> {
    let otherwise = host.emit(Op::JumpIfFalse { src: cond, target: usize::MAX });
    body.eval(host, cx)?;
    let after = host.emit(Op::Jump(usize::MAX));

    host.patch_to_here(otherwise);
    alt.eval(host, cx)?;
    host.patch_to_here(after);

    host.reset_regs();
    Ok(cond)
}
