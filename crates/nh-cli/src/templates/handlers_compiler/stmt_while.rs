//! Both `cond` and `body` are lazy: the condition's code has to be emitted
//! *inside* the loop so it is re-executed each time round.
use nh_runtime::Shared;
use nh_runtime::{Ctx, Result};
use crate::generated::ast::{Block, Expr};
use crate::generated::dispatch::Eval;
use crate::{Interp, Reg};

pub fn run(host: &mut Interp, cond: &Shared<Expr>, body: &Shared<Block>, cx: &mut Ctx) -> Result<Reg> {
    let top = host.here();
    let c = cond.eval(host, cx)?;
    let to_end = host.emit_jump_if_false(c);
    host.free(c);

    host.enter_loop();
    body.eval(host, cx)?;
    host.emit_jump_to(top);

    host.patch_to_here(to_end);
    host.exit_loop(top);
    Ok(host.next_reg())
}
