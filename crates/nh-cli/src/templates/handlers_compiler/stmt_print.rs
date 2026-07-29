use nh_runtime::{Ctx, Result};
use crate::{Interp, Reg};

pub fn run(host: &mut Interp, value: Reg, _cx: &mut Ctx) -> Result<Reg> {
    host.emit_print(value);
    host.free(value);
    Ok(host.next_reg())
}
