use nh_runtime::{Ctx, Result};
use crate::{Interp, Reg};

pub fn run(host: &mut Interp, value: Option<Reg>, _cx: &mut Ctx) -> Result<Reg> {
    host.emit_return(value);
    if let Some(v) = value {
        host.free(v);
    }
    Ok(host.next_reg())
}
