//! `if cond { .. } else { .. }`
//!
//! `cond` arrives evaluated; the two branches do not. That is what `lazy`
//! buys: exactly one of them runs.
use nh_runtime::{Ctx, Result, Shared};
use crate::generated::ast::{Block, ElseTail};
use crate::generated::dispatch::Eval;
use crate::{Interp, Value};

pub fn run(
    host: &mut Interp,
    cond: Value,
    then: &Shared<Block>,
    otherwise: Option<&Shared<ElseTail>>,
    cx: &mut Ctx,
) -> Result<Value> {
    if host.is_true(&cond) {
        then.eval(host, cx)
    } else if let Some(tail) = otherwise {
        tail.eval(host, cx)
    } else {
        Ok(Value::Null)
    }
}
