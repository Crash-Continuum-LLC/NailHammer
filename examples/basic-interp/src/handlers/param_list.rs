//! Handler for `param_list` — the parameter names in a `FUNCTION` header.
//!
//! From `rule param_list = head:IDENT tail:more_param* -> params;`
//!
//! Never actually called. The `FUNCTION` handler binds this node `lazy` and
//! reads the names straight off the AST, because a parameter name is not an
//! expression and there is nothing here to evaluate. A handler still has to
//! exist — the trait requires one per alternative — and saying so is better
//! than a body that pretends to do something.

use nh_runtime::{Ctx, Result};

use crate::{Interp, Value};

pub fn run(_host: &mut Interp, _head: &nh_runtime::Name, _tail: Vec<Value>, cx: &mut Ctx) -> Result<Value> {
    cx.err("a parameter list is not a value")
}
