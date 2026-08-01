//! `name:IDENT "[" index:expr "]" -> elem place`
//!
//! A *read* of an element. The `place` marker means the same alternative is
//! also an assignment target, and that half is generated — see `assign` in
//! `generated/vm_operators.rs`.
use nh_runtime::{Ctx, Result};
use nh_vm::{Op, Reg};

use crate::Interp;

pub fn run(host: &mut Interp, name: &str, index: Reg, _cx: &mut Ctx) -> Result<Reg> {
    let seq = host.read_var(name);
    let dst = host.reuse(&[seq, index]);
    host.emit(Op::Index { dst, seq, idx: index });
    Ok(dst)
}
