//! Handler for `stmt_bind`.
//!
//! From `| "let" name:IDENT "=" value:expr ";" -> bind`

use nh_runtime::{Ctx, Result};
{{name_import}}

use crate::Interp;

pub fn run(host: &mut Interp, name: {{name_ty}}, _value: (), _cx: &mut Ctx) -> Result<()> {
    // `value` was emitted before this ran, so its result is on the stack.
    host.emit_store(name{{key}});
    host.emit_pop();
    Ok(())
}
