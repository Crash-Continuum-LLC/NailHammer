//! `fn name(a, b) { .. }`
//!
//! The parameters arrive as **names**, not values — there is nothing to
//! evaluate about `a` in a definition. That is why a definition and a call
//! cannot share a rule.

use nh_runtime::{Ctx, Result, Shared};
use crate::generated::ast::Block;
use crate::{Function, Interp, Value};

pub fn run(
    host: &mut Interp,
    name: &str,
    params: &[String],
    body: &Shared<Block>,
    cx: &mut Ctx,
) -> Result<Value> {
    for (i, p) in params.iter().enumerate() {
        if params[..i].contains(p) {
            return cx.err(format!("parameter `{p}` is bound twice"));
        }
    }
    host.define(
        name,
        Function {
            params: params.to_vec(),
            // `lazy` gave us an owned node, so keeping it costs a refcount.
            body: body.clone(),
        },
    );
    Ok(Value::Null)
}
