//! Handler for `primary_var`.
//!
//! From `| name:IDENT -> var place`
//!
//! Reading a variable is a Load. Writing one is a Store, and lives in
//! `Operators::assign` — that split is what `place` in the grammar buys you.

use nh_runtime::{Ctx, Result};

use crate::Interp;

pub fn run(host: &mut Interp, name: &str, _cx: &mut Ctx) -> Result<()> {
    host.emit_load(name);
    Ok(())
}
