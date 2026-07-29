//! Handler for `line` — one statement and the newline that ends it.
//!
//! Only the line-oriented style has this. Nothing to emit: the statement
//! already did.

use nh_runtime::{Ctx, Result};

use crate::Interp;

pub fn run(_host: &mut Interp, _body: (), _cx: &mut Ctx) -> Result<()> {
    Ok(())
}
