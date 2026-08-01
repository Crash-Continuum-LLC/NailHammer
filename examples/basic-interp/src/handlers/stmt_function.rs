//! Handler for `stmt_function` — `FUNCTION f(a, b) ... END FUNCTION`.
//!
//! Two different uses of `lazy` in one signature, and the contrast is the
//! interesting part:
//!
//! * `lazy body:line*` defers **evaluation** — the body runs at each call.
//! * `lazy params:param_list?` is not about deferral at all. Parameter names
//!   are not expressions; there is nothing to evaluate. It is how a handler
//!   asks for the node's *structure* instead of its value.


use nh_runtime::{Ctx, Name, Result, Shared};

use crate::generated::ast::{Line, ParamList};
use crate::{Function, Interp, Value};

pub fn run(
    host: &mut Interp,
    name: &Name,
    params: Option<&Shared<ParamList>>,
    body: &[Shared<Line>],
    cx: &mut Ctx,
) -> Result<Value> {
    if host.funcs.contains_key(name.key()) {
        return cx.err(format!("`FUNCTION {}` is already defined", name.text()));
    }

    let mut names = Vec::new();
    if let Some(p) = params {
        names.push(p.head.key().to_string());
        for more in &p.tail {
            names.push(more.name.key().to_string());
        }
    }

    // Two parameters with one name would make the second silently win.
    for (i, n) in names.iter().enumerate() {
        if names[..i].contains(n) {
            return cx.err(format!("parameter `{n}` is bound twice"));
        }
    }

    host.funcs.insert(
        name.key().to_string(),
        Function {
            params: names,
            body: body.to_vec(),
        },
    );
    Ok(Value::Nothing)
}
