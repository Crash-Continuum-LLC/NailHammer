//! Handler for `program`.

use nh_runtime::{Ctx, Result};

use crate::{Interp, Value};

pub fn run(_host: &mut Interp, stmts: Vec<Value>, _cx: &mut Ctx) -> Result<Value> {
    Ok(stmts.into_iter().last().unwrap_or(Value::Bool(true)))
}
