//! `while cond { .. }`
//!
//! Both operands are `lazy`, because a loop re-tests its condition. An
//! evaluated `cond` would be one boolean, decided once.
use nh_runtime::{Ctx, Result, Shared};
use crate::generated::ast::{Block, Expr};
use crate::generated::dispatch::Eval;
use crate::{Interp, Value};

pub fn run(
    host: &mut Interp,
    cond: &Shared<Expr>,
    body: &Shared<Block>,
    cx: &mut Ctx,
) -> Result<Value> {
    let mut last = Value::Null;
    loop {
        let test = cond.eval(host, cx)?;
        if !host.is_true(&test) {
            return Ok(last);
        }
        last = body.eval(host, cx)?;
    }
}
