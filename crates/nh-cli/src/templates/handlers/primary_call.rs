//! Handler for `primary_call` — calling a function.
//!
//! The arguments arrive **already evaluated, left to right**, which is what
//! `first`/`rest` are: `f()` is `None` + `[]`, `f(a, b, c)` is `Some(a)` +
//! `[b, c]`. Splitting them that way is how the grammar gets a comma-separated
//! list without a trailing-comma hole.

use nh_runtime::{Ctx, Result};
{{name_import}}

use crate::{Interp, Value};

pub fn run(
    host: &mut Interp,
    name: {{name_ty}},
    first: Option<Value>,
    rest: Vec<Value>,
    cx: &mut Ctx,
) -> Result<Value> {
    let mut args = Vec::with_capacity(rest.len() + 1);
    args.extend(first);
    args.extend(rest);
    host.call(name{{key}}, args, cx)
}
