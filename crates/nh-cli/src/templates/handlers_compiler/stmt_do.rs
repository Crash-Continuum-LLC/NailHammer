//! Body first, then the test. `continue` goes to the test, not the top.
use nh_runtime::Shared;
use nh_runtime::{Ctx, Result};
use crate::generated::ast::{Block, Expr};
use crate::generated::dispatch::Eval;
use crate::{Interp, Reg};

pub fn run(host: &mut Interp, body: &Shared<Block>, cond: &Shared<Expr>, cx: &mut Ctx) -> Result<Reg> {
    let top = host.here();
    host.enter_loop();
    body.eval(host, cx)?;

    let test = host.here();
    let c = cond.eval(host, cx)?;
    let back = host.emit_jump_if_true(c);
    host.free(c);
    host.patch_to(back, top);

    host.exit_loop(test);
    Ok(host.next_reg())
}
