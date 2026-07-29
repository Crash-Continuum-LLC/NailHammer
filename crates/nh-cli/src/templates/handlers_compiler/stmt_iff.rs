//! `if cond { .. } else { .. }`
//!
//! `cond` arrives in a register — its code was emitted before this ran. The
//! branches are `lazy`, so this decides where each one goes.
use std::rc::Rc;
use nh_runtime::{Ctx, Result};
use crate::generated::ast::{Block, ElseTail};
use crate::generated::dispatch::Eval;
use crate::{Interp, Reg};

pub fn run(
    host: &mut Interp,
    cond: Reg,
    then: &Rc<Block>,
    otherwise: Option<&Rc<ElseTail>>,
    cx: &mut Ctx,
) -> Result<Reg> {
    let to_else = host.emit_jump_if_false(cond);
    host.free(cond);
    then.eval(host, cx)?;

    match otherwise {
        None => host.patch_to_here(to_else),
        Some(tail) => {
            let to_end = host.emit_jump();
            host.patch_to_here(to_else);
            tail.eval(host, cx)?;
            host.patch_to_here(to_end);
        }
    }
    Ok(host.next_reg())
}
