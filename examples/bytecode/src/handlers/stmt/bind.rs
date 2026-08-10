//! Handler for `stmt_bind`.
//!
//! From `| "let" name:IDENT "=" value:expr ";" -> bind`

use nh_runtime::{Ctx, Result};

use crate::Interp;

pub fn run(host: &mut Interp, name: &str, _value: (), _cx: &mut Ctx) -> Result<()> {
    // `value` was emitted before this ran, so its result is on the stack.
    host.emit_store(name);
    host.emit_pop();
    Ok(())
}
