//! A literal: take a register and put the constant in it.
use nh_runtime::{Ctx, Result};
use crate::{Interp, Reg};

pub fn run(host: &mut Interp, digits: &str, cx: &mut Ctx) -> Result<Reg> {
    match digits.parse::<f64>() {
        Ok(n) => Ok(host.emit_const(n)),
        Err(_) => cx.err("not a valid number"),
    }
}
