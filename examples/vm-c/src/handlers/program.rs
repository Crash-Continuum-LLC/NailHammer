//! `SOI body:stmt+ EOI -> program`
//!
//! The statements emitted themselves; all that is left is to stop the machine.
use nh_runtime::{Ctx, Result};
use nh_vm::Reg;

use crate::Interp;

pub fn run(host: &mut Interp, body: Vec<Reg>, _cx: &mut Ctx) -> Result<Reg> {
    host.finish();
    Ok(body.last().copied().unwrap_or(0))
}
