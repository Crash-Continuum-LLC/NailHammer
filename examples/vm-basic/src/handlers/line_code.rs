//! `body:stmt EOL+ -> code`
//!
//! A line holding one statement. The statement emitted itself; the newline is
//! punctuation. It needs a label rather than `-> pass` because the alternative
//! matches two things — the statement and its terminator — and `pass` is for
//! delegating to exactly one child.
use nh_runtime::{Ctx, Result};
use nh_vm::Reg;

use crate::Interp;

pub fn run(_host: &mut Interp, body: Reg, _cx: &mut Ctx) -> Result<Reg> {
    Ok(body)
}
