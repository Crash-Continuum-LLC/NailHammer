//! Handler for `primary_var`.
//!
//! From `| name:IDENT -> var`

use nh_runtime::{Ctx, Name, Result};

use crate::{Interp, Value};

pub fn run(host: &mut Interp, name: &Name, cx: &mut Ctx) -> Result<Value> {
    match host.lookup(name.key()) {
        Some(v) => Ok(v.clone()),
        // `.text()` in the message: report what the programmer typed, not the
        // folded form they never wrote.
        None => cx.err(format!("undefined variable `{}`", name.text())),
    }
}
