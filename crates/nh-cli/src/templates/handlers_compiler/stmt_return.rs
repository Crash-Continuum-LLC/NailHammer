//! Handler for `stmt_return`.
//!
//! The interpreter raises a signal and lets `?` unwind to the call. There is
//! nothing to unwind here, so this emits the instruction that will do the
//! unwinding when the program eventually runs.

use nh_runtime::{Ctx, Result};

use crate::Interp;

pub fn run(host: &mut Interp, value: Option<()>, _cx: &mut Ctx) -> Result<()> {
    // `value:expr?`, so a bare `return` has emitted nothing and the machine
    // needs something to return.
    if value.is_none() {
        host.emit_push(0.0);
    }
    host.emit_return();
    Ok(())
}
