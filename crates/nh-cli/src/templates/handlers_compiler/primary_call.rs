//! Handler for `primary_call` — calling a function.
//!
//! The arguments emitted themselves, left to right, before this ran — so they
//! are already on the stack in the order the callee expects. Eager bindings
//! give a compiler its calling convention for free.
//!
//! The callee is looked up by name at *run* time rather than patched here, so
//! a function can be called before it is defined, and can call itself.

use nh_runtime::{Ctx, Result};

use crate::Interp;

pub fn run(
    host: &mut Interp,
    name: &str,
    first: Option<()>,
    rest: Vec<()>,
    _cx: &mut Ctx,
) -> Result<()> {
    let argc = usize::from(first.is_some()) + rest.len();
    host.emit_call(name, argc);
    Ok(())
}
