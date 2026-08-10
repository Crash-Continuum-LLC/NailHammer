//! `"print" value:expr ";" -> print`
//!
//! `value` is already in a register: its code was emitted before this ran.
use nh_runtime::{Ctx, Result};
use nh_vm::{Op, Reg};

use nh_vm::Emitter;

use crate::Interp;

pub fn run(host: &mut Interp, value: Reg, _cx: &mut Ctx) -> Result<Reg> {
    host.emit(Op::Print { src: value });
    host.reset_regs();
    Ok(value)
}
