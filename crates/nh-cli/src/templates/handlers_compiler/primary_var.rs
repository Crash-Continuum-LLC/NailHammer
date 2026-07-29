//! Reading a variable.
//!
//! A local **emits nothing**: it already lives in a register, and this hands
//! back the slot. Only a global costs an instruction and a hash lookup.

use nh_runtime::{Ctx, Result};
{{name_import}}use crate::{Interp, Reg};

pub fn run(host: &mut Interp, name: {{name_ty}}, _cx: &mut Ctx) -> Result<Reg> {
    Ok(host.read_var(name{{key}}))
}
