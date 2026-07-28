//! Handler for `line` — one statement, its optional line number, and the
//! newlines after it.
//!
//! From `rule line = label:NUMBER? body:stmt EOL* -> line;`
//!
//! The label is not used here: `program` reads it off the AST to build its jump
//! table, which it has to do *before* any line runs.

use nh_runtime::{Ctx, Result};

use crate::{Interp, Value};

pub fn run(_host: &mut Interp, _label: Option<&str>, body: Value, _cx: &mut Ctx) -> Result<Value> {
    Ok(body)
}
