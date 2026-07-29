//! Handler for `primary_var`.
//!
//! From `| name:IDENT -> var place`

use nh_runtime::{Ctx, Result};
{{name_import}}

use crate::{Interp, Value};

pub fn run(host: &mut Interp, name: {{name_ty}}, cx: &mut Ctx) -> Result<Value> {
    match host.get(name{{key}}) {
        Some(v) => Ok(v.clone()),
        None => cx.err(format!("undefined variable `{name}`")),
    }
}
