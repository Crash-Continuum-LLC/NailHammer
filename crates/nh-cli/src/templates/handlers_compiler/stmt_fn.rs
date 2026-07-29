//! A function definition.
//!
//! `enter_function` gives the body its own register file: parameters are slots
//! `0..n`, and every local the body declares takes the next slot. The caller
//! copies arguments straight into those slots, so a call involves no names.

use std::rc::Rc;
use nh_runtime::{Ctx, Result};
{{name_import}}use crate::generated::ast::{Block, ParamList};
use crate::generated::dispatch::Eval;
use crate::{Interp, Reg};

pub fn run(
    host: &mut Interp,
    name: {{name_ty}},
    params: Option<&Rc<ParamList>>,
    body: &Rc<Block>,
    cx: &mut Ctx,
) -> Result<Reg> {
    let names = param_names(params);

    let over = host.emit_jump();
    let addr = host.here();

    let saved = host.enter_function(&names);
    // Registered before the body is emitted, so a function can call itself.
    host.define_fn(name{{key}}, addr, names.len(), 0);
    body.eval(host, cx)?;
    host.emit_return(None);
    let frame = host.exit_function(saved);
    host.define_fn(name{{key}}, addr, names.len(), frame);

    host.patch_to_here(over);
    Ok(host.next_reg())
}

fn param_names(params: Option<&Rc<ParamList>>) -> Vec<String> {
    match params {
        None => Vec::new(),
        Some(list) => std::iter::once(list.first{{key}}.to_string())
            .chain(list.rest.iter().map(|p| p.name{{key}}.to_string()))
            .collect(),
    }
}
