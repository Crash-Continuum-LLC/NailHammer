//! Handler for `stmt_fn` — a function definition.
//!
//! A definition is not a call, and for a compiler it is not even code that
//! runs where it is written. The body is emitted **in place**, with a jump
//! around it so that falling off the end of the previous statement does not
//! walk straight into it.
//!
//! ```text
//! fn f(a, b) { body }
//!
//!   Jump over · f: Store b · Pop · Store a · Pop · <body> · PushUnit · Return · over:
//! ```
//!
//! Parameters are stored in reverse because the last argument is on top of the
//! stack — the caller pushed them left to right.

use std::rc::Rc;

use nh_runtime::{Ctx, Result};
{{name_import}}

use crate::generated::ast::{Block, ParamList};
use crate::generated::dispatch::Eval;
use crate::{FnInfo, Interp};

pub fn run(
    host: &mut Interp,
    name: {{name_ty}},
    params: Option<&Rc<ParamList>>,
    body: &Rc<Block>,
    cx: &mut Ctx,
) -> Result<()> {
    let names = param_names(params);

    let over = host.emit_jump();
    let addr = host.here();

    host.fns.insert(
        name{{key}}.to_string(),
        FnInfo {
            addr,
            arity: names.len(),
        },
    );

    for p in names.iter().rev() {
        host.emit_store(p);
        host.emit_pop();
    }

    body.eval(host, cx)?;

    // Falling off the end returns nothing. A `return` inside the body emitted
    // its own `Return` and never reaches here.
    host.emit_push(0.0);
    host.emit_return();

    host.patch_to_here(over);
    Ok(())
}

fn param_names(params: Option<&Rc<ParamList>>) -> Vec<String> {
    match params {
        None => Vec::new(),
        Some(list) => std::iter::once(list.first{{key}}.to_string())
            .chain(list.rest.iter().map(|p| p.name{{key}}.to_string()))
            .collect(),
    }
}
