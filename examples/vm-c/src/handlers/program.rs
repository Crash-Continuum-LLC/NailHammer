//! `SOI body:stmt+ EOI -> program`
//!
//! The statements emitted themselves, so there is nothing to do here.
//!
//! Stopping the machine is `Emitter::finish`, and the *driver* calls it — not
//! this handler. `finish` takes the code rather than borrowing it, so calling
//! it twice leaves the second caller with an empty program.
use nh_runtime::{Ctx, Result};
use nh_vm::Reg;

use crate::Interp;

pub fn run(_host: &mut Interp, body: Vec<Reg>, _cx: &mut Ctx) -> Result<Reg> {
    Ok(body.last().copied().unwrap_or(0))
}
