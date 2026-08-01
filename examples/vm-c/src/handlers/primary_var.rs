//! `name:IDENT -> var place`
//!
//! A read. The `place` marker is what lets the same alternative be assigned to.
use nh_runtime::{Ctx, Result};
use nh_vm::Reg;

use nh_vm::Emitter;

use crate::Interp;

pub fn run(host: &mut Interp, name: &str, _cx: &mut Ctx) -> Result<Reg> {
    Ok(host.read_var(name))
}
