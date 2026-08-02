//! `"return" value:expr ";" -> ret`
use nh_runtime::{Ctx, Result};
use nh_vm::{Emitter, Op, Reg};

use crate::Interp;

pub fn run(host: &mut Interp, value: Reg, _cx: &mut Ctx) -> Result<Reg> {
    host.emit(Op::Return { src: value });
    Ok(value)
}
