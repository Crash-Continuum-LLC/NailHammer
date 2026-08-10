//! Handler for `stmt_define` — `SUB name ... END SUB`.
//!
//! From `| "SUB" name:IDENT EOL* lazy body:line* "END" "SUB" -> define`
//!
//! **The body is kept, not run.** `lazy` means it arrives unevaluated, and
//! because the tree is owned it can be cloned onto the interpreter and run at
//! every later `CALL`. Under the borrowed `Deferred` of M2–M6 this handler
//! could not have been written: the body could not outlive the call that
//! received it (DESIGN.md §9).


use nh_runtime::{Ctx, Name, Result, Shared};

use crate::generated::ast::Line;
use crate::{Interp, Value};

pub fn run(host: &mut Interp, name: &Name, body: &[Shared<Line>], cx: &mut Ctx) -> Result<Value> {
    if host.subs.contains_key(name.key()) {
        return cx.err(format!("`SUB {}` is already defined", name.text()));
    }
    // Cloning a slice of `Shared` copies pointers, not the program.
    host.subs.insert(name.key().to_string(), body.to_vec());
    Ok(Value::Nothing)
}
