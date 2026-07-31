//! `name:IDENT "=" value:expr ";" -> assign`
use nh_runtime::{Ctx, Result};
use nh_vm::{Op, Reg};

use crate::Interp;

pub fn run(host: &mut Interp, name: &str, value: Reg, _cx: &mut Ctx) -> Result<Reg> {
    let slot = host.slot_of(name);
    host.emit(Op::StoreGlobal { slot, src: value });
    host.reset_regs();
    Ok(value)
}
