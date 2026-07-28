//! Handler for `primary_var`.
//!
//! From `| name:IDENT -> var place`

use nh_runtime::{Ctx, Result};

use crate::{Interp, Value};

pub fn run(host: &mut Interp, name: &str, cx: &mut Ctx) -> Result<Value> {
    match host.vars.get(name) {
        Some(v) => Ok(v.clone()),
        None => cx.err(format!("undefined variable `{name}`")),
    }
}
