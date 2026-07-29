//! Handler for `block` — a group of statements.
//!
//! Each statement emitted its own code before this ran, in order, so the block
//! is already in the instruction stream and there is nothing left to do.

use nh_runtime::{Ctx, Result};

use crate::Interp;

pub fn run(_host: &mut Interp, _stmts: Vec<()>, _cx: &mut Ctx) -> Result<()> {
    Ok(())
}
