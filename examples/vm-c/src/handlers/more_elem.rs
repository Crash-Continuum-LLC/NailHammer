//! `"," value:expr -> next`
//!
//! Nothing to emit: the element evaluated itself into the next register, which
//! is exactly where `NewArray` will look for it.
use nh_runtime::{Ctx, Result};
use nh_vm::Reg;

use crate::Interp;

pub fn run(_host: &mut Interp, value: Reg, _cx: &mut Ctx) -> Result<Reg> {
    Ok(value)
}
