//! `let x = value`
//!
//! Inside a function this gives `x` a slot — and the value is usually already
//! in it, because a statement starts with no temporaries live and the
//! allocator hands out the bottom first. So it often emits nothing at all.

use nh_runtime::{Ctx, Result};
{{name_import}}use crate::{Interp, Reg};

pub fn run(host: &mut Interp, name: {{name_ty}}, value: Reg, _cx: &mut Ctx) -> Result<Reg> {
    let home = host.write_var(name{{key}}, value);
    // A no-op for a slot, which lives below the temporaries and is never freed.
    host.free(home);
    Ok(host.next_reg())
}
