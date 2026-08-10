//! Handler for `stmt_print`.
//!
//! From `| "print" value:expr ";" -> print`

use nh_runtime::{Ctx, Result};

use crate::Interp;

pub fn run(host: &mut Interp, _value: (), _cx: &mut Ctx) -> Result<()> {
    host.emit_print();
    Ok(())
}
