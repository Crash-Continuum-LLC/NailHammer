//! Handler for `primary_var`.

use nh_runtime::{Ctx, Result};

use crate::{Interp, Value};

pub fn run(host: &mut Interp, name: &str, cx: &mut Ctx) -> Result<Value> {
    match host.vars.get(name) {
        Some(v) => Ok(v.clone()),
        // No span threading: dispatch already pushed this node's location.
        None => cx.err(format!("undefined variable `{name}`")),
    }
}
