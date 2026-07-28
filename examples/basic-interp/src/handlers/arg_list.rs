//! Handler for `arg_list` — the arguments at a call site.
//!
//! From `rule arg_list = head:expr tail:more_arg* -> args;`
//!
//! Arguments *are* expressions, so they arrive evaluated. Contrast
//! `param_list`, whose contents are names and never evaluated at all.

use nh_runtime::{Ctx, Result};

use crate::{Interp, Value};

pub fn run(_host: &mut Interp, head: Value, tail: Vec<Value>, _cx: &mut Ctx) -> Result<Value> {
    let mut args = vec![head];
    args.extend(tail);
    Ok(Value::List(args))
}
