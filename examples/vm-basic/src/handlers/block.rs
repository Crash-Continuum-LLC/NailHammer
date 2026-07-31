//! `EOL+ body:line+ -> block`
//!
//! Nothing to emit: the statements emitted themselves as they were evaluated.
//! The block exists so `if` and `while` have one thing to hold `lazy`.
use nh_runtime::{Ctx, Result};
use nh_vm::Reg;

use crate::Interp;

pub fn run(host: &mut Interp, body: Vec<Reg>, _cx: &mut Ctx) -> Result<Reg> {
    Ok(body.last().copied().unwrap_or_else(|| host.alloc()))
}
