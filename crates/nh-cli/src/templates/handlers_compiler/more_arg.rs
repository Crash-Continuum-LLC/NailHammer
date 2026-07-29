use nh_runtime::{Ctx, Result};
use crate::{Interp, Reg};

pub fn run(_host: &mut Interp, value: Reg, _cx: &mut Ctx) -> Result<Reg> {
    Ok(value)
}
