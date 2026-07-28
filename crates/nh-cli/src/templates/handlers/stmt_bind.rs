//! Handler for `stmt_bind`.
//!
//! From `| "let" name:IDENT "=" value:expr ";" -> bind`

use nh_runtime::{Ctx, Result};

use crate::{Interp, Value};

pub fn run(host: &mut Interp, name: &str, value: Value, _cx: &mut Ctx) -> Result<Value> {
    // The parameters are the bindings, in grammar order. Reorder the rule and
    // nothing here changes; rename a binding and this stops compiling.
    host.vars.insert(name.to_string(), value.clone());
    Ok(value)
}
