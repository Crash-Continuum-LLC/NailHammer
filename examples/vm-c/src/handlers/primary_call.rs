//! `name:IDENT "(" args:exprs ")" -> call`
//!
//! `args` is the register the *last* argument landed in; the arguments before
//! it are already below it, because the allocator hands out registers in stack
//! discipline. So a call finds its arguments contiguous with nobody arranging
//! them — which is the whole calling convention.
use nh_runtime::{Ctx, Result};
use nh_vm::{Emitter, Op, Reg};

use crate::Interp;

pub fn run(host: &mut Interp, name: &str, args: Reg, _cx: &mut Ctx) -> Result<Reg> {
    let base = args;
    let argc = host.argc;
    let dst = host.reuse(&[base]);
    host.emit(Op::Call {
        dst,
        base,
        argc,
        // One key, one spelling. A case-folding language would fold the key and
        // leave `shown` as the user typed it; this one does not fold.
        key: name.to_string(),
        shown: name.to_string(),
    });
    Ok(dst)
}
