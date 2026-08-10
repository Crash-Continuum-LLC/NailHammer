//! `"LET" name:IDENT "[" index:expr "]" "=" value:expr -> setelem`
//!
//! BASIC keeps `LET` a statement and `=` a comparison, so element assignment
//! needs its own form here — where the C twin gets it from `Place`, because it
//! binds `=` as an operator. Same instruction at the end of both paths.
use nh_runtime::{Ctx, Result};
use nh_vm::{Emitter, Op, Reg};

use crate::Interp;

pub fn run(host: &mut Interp, name: &str, index: Reg, value: Reg, _cx: &mut Ctx) -> Result<Reg> {
    let seq = host.read_var(name);
    host.emit(Op::SetIndex { seq, idx: index, src: value });
    host.reset_regs();
    Ok(value)
}
