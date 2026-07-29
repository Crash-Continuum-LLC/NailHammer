//! Handler for `stmt_fn` — a function definition.
//!
//! Defining is not calling: this stores the body and returns. `body` is `lazy`
//! precisely so that it is *not* run here — it runs once per call, later, from
//! `primary_call`.
//!
//! `params` is `lazy` for a different reason. Every other binding arrives
//! evaluated, and evaluating a parameter name would look it up as a variable
//! that does not exist yet. So it arrives as a node, and this walks it.

use std::rc::Rc;

use nh_runtime::{Ctx, Result};
{{name_import}}

use crate::generated::ast::{Block, ParamList};
use crate::{Function, Interp, Value};

pub fn run(
    host: &mut Interp,
    name: {{name_ty}},
    params: Option<&Rc<ParamList>>,
    body: &Rc<Block>,
    cx: &mut Ctx,
) -> Result<Value> {
    let _ = cx;
    host.fns.insert(
        name{{key}}.to_string(),
        Function {
            params: param_names(params),
            body: Rc::clone(body),
        },
    );
    Ok(Value::Unit)
}

/// Walks the unevaluated list. The node's fields are the grammar's bindings,
/// so this reads the same as the rule it came from:
///
/// ```text
/// rule param_list = first:IDENT rest:more_param* -> list;
/// rule more_param = "," name:IDENT -> one;
/// ```
fn param_names(params: Option<&Rc<ParamList>>) -> Vec<String> {
    match params {
        None => Vec::new(),
        Some(list) => std::iter::once(list.first{{key}}.to_string())
            .chain(list.rest.iter().map(|p| p.name{{key}}.to_string()))
            .collect(),
    }
}
