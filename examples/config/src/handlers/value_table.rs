//! Handler for `value_table`.
//!
//! From `| "{" fields:entry* "}" -> table`

use nh_runtime::{Ctx, Result};

use crate::{Interp, Value};

pub fn run(_host: &mut Interp, fields: Vec<Value>, cx: &mut Ctx) -> Result<Value> {
    let mut out = Vec::new();
    for field in fields {
        match field.into_field() {
            Some(pair) => out.push(pair),
            None => return cx.err("expected a key/value entry"),
        }
    }
    Ok(Value::Table(out))
}
