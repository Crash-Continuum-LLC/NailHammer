//! Handler for `program`.
//!
//! From `rule program = SOI stmts:stmt* EOI -> doc;`
//!
//! Each statement emitted its own code before this ran, so there is nothing
//! left to do. An interpreter would have a `Vec<Value>` here; this has a
//! `Vec<()>`, because the results are in `host.code`.

use nh_runtime::{Ctx, Result};

use crate::Interp;

pub fn run(_host: &mut Interp, _stmts: Vec<()>, _cx: &mut Ctx) -> Result<()> {
    Ok(())
}
