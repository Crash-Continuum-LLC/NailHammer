//! `EOL -> blank`
//!
//! A line with nothing on it. BASIC's line-oriented syntax makes this a node;
//! the C twin has no equivalent because braces do not care about newlines.
//! Nothing to emit — which is the point of it being a handler rather than a
//! special case somewhere in the machine.
use nh_runtime::{Ctx, Result};
use nh_vm::Reg;

use crate::Interp;

pub fn run(_host: &mut Interp, _cx: &mut Ctx) -> Result<Reg> {
    Ok(0)
}
