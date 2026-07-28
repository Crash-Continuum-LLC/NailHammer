//! Handler for `stmt_bind` — `let name = expr;`

use nh_runtime::{Ctx, Result};

use crate::{Interp, Value};

pub fn run(host: &mut Interp, name: &str, value: Value, _cx: &mut Ctx) -> Result<Value> {
    host.vars.insert(name.to_string(), value.clone());
    Ok(value)
}
