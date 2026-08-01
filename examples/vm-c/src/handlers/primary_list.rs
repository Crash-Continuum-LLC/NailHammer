//! `"[" first:expr rest:more_elem* "]" -> list`
//!
//! `NewArray` takes its elements from *contiguous* registers, which they
//! already are: the allocator hands them out in stack discipline, so evaluating
//! the elements in order leaves them side by side with nobody arranging it.
use nh_runtime::{Ctx, Result};
use nh_vm::{Op, Reg};

use nh_vm::Emitter;

use crate::Interp;

pub fn run(host: &mut Interp, first: Reg, rest: Vec<Reg>, _cx: &mut Ctx) -> Result<Reg> {
    let len = 1 + rest.len();
    let dst = host.alloc();
    host.emit(Op::NewArray { dst, base: first, len });
    Ok(dst)
}
