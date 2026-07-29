//! Handler for `stmt_while`.
//!
//! Both `cond` and `body` are `lazy`, and both have to be. A loop condition is
//! re-tested every iteration, so evaluating it once — which is what a plain
//! binding would do — would give you a loop that never stops or never starts.

use std::rc::Rc;

use nh_runtime::{Ctx, Error, Result};

use crate::generated::ast::{Block, Expr};
use crate::generated::dispatch::{Eval, Values};
use crate::{Interp, Value};

pub fn run(host: &mut Interp, cond: &Rc<Expr>, body: &Rc<Block>, cx: &mut Ctx) -> Result<Value> {
    loop {
        let test = cond.eval(host, cx)?;
        if !host.truthy(&test) {
            break;
        }

        // `break` and `continue` arrive as signals. `?` already unwinds exactly
        // the way a non-local jump needs to, so raising one costs nothing and
        // catching one is this match — no flag, no sentinel return value.
        match body.eval(host, cx) {
            Ok(_) => {}
            Err(Error::Signal { label: "break", .. }) => break,
            Err(Error::Signal { label: "continue", .. }) => continue,
            Err(other) => return Err(other),
        }
    }
    Ok(Value::Unit)
}
