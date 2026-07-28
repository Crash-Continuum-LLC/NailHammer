//! Handler for `more_param` — one `, name` after the first parameter.
//!
//! From `rule more_param = "," name:IDENT -> next;`
//!
//! Unreachable for the same reason as [`super::param_list`]: the `FUNCTION`
//! handler reads these names off the AST rather than evaluating them.

use nh_runtime::{Ctx, Result};

use crate::{Interp, Value};

pub fn run(_host: &mut Interp, _name: &nh_runtime::Name, cx: &mut Ctx) -> Result<Value> {
    cx.err("a parameter name is not a value")
}
