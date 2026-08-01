//! `value:expr ";" -> eval`
//!
//! An expression statement. Assignment is an *operator* in this grammar, so
//! `x = 1;` arrives here — the store was emitted by the generated `assign`,
//! and there is nothing left to do but drop the temporaries.
use nh_runtime::{Ctx, Result};
use nh_vm::Reg;

use nh_vm::Emitter;

use crate::Interp;

pub fn run(host: &mut Interp, value: Reg, _cx: &mut Ctx) -> Result<Reg> {
    host.reset_regs();
    Ok(value)
}
