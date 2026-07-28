//! Handler for `program`.
//!
//! From `rule program = SOI stmts:stmt* EOI -> doc;`

use nh_runtime::{Ctx, Result};

use crate::Interp;

pub fn run(_host: &mut Interp, _stmts: Vec<()>, _cx: &mut Ctx) -> Result<()> {
    Ok(())
}
