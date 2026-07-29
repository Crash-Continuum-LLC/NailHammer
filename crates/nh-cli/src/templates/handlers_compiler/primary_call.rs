//! The arguments emitted themselves, left to right, into an allocator that
//! hands out the top of the register file — so they are already in consecutive
//! registers, which is exactly the calling convention.
use nh_runtime::{Ctx, Result};
{{name_import}}use crate::{Interp, Reg};

pub fn run(
    host: &mut Interp,
    name: {{name_ty}},
    first: Option<Reg>,
    rest: Vec<Reg>,
    _cx: &mut Ctx,
) -> Result<Reg> {
    let mut args: Vec<Reg> = Vec::with_capacity(rest.len() + 1);
    args.extend(first);
    args.extend(rest);
    Ok(host.emit_call(name, &args))
}
