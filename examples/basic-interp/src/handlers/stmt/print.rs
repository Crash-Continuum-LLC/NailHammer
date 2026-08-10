//! Handler for `stmt_print` — `PRINT a, b, c`.
//!
//! From `| "PRINT" head:expr tail:more_print* -> print`

use nh_runtime::{Ctx, Result};

use crate::{Interp, Value};

pub fn run(host: &mut Interp, head: Value, tail: Vec<Value>, _cx: &mut Ctx) -> Result<Value> {
    let mut line = head.to_string();
    for item in tail {
        line.push('\t');
        line.push_str(&item.to_string());
    }
    host.output.push(line);
    Ok(Value::Nothing)
}
