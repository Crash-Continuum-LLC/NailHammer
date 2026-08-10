//! Handler for `primary_var`.
//!
//! From `| name:IDENT -> var place`

use nh_runtime::{Ctx, Result};

use crate::Interp;

pub fn run(host: &mut Interp, name: &str, _cx: &mut Ctx) -> Result<()> {
    host.emit_load(name);
    Ok(())
}
