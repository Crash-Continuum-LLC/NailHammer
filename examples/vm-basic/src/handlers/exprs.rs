//! `first:expr rest:more_elem* -> some`
//!
//! Hands back where the arguments start and how many there are. They are
//! already contiguous — evaluating them in order put them side by side.
use nh_runtime::{Ctx, Result};
use nh_vm::Reg;

use crate::Interp;

pub fn run(host: &mut Interp, first: Reg, rest: Vec<Reg>, _cx: &mut Ctx) -> Result<Reg> {
    host.argc = 1 + rest.len();
    Ok(first)
}
