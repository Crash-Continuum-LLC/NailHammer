//! A group of statements. Already emitted, in order.
use nh_runtime::{Ctx, Result};
use crate::{Interp, Reg};

pub fn run(host: &mut Interp, _stmts: Vec<Reg>, _cx: &mut Ctx) -> Result<Reg> {
    Ok(host.next_reg())
}
