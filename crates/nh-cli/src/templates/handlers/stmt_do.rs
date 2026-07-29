//! Handler for `stmt_do` — a loop that runs its body before testing.
//!
//! The only difference from `while` is the order of the two lines, which is
//! the whole point of having both.

use nh_runtime::Shared;

use nh_runtime::{Ctx, Error, Result};

use crate::generated::ast::{Block, Expr};
use crate::generated::dispatch::{Eval, Values};
use crate::{Interp, Value};

pub fn run(host: &mut Interp, body: &Shared<Block>, cond: &Shared<Expr>, cx: &mut Ctx) -> Result<Value> {
    loop {
        match body.eval(host, cx) {
            Ok(_) => {}
            Err(Error::Signal { label: "break", .. }) => break,
            // Not `continue`: the test still has to run, or `continue` would
            // skip it and the loop could never end.
            Err(Error::Signal { label: "continue", .. }) => {}
            Err(other) => return Err(other),
        }

        let test = cond.eval(host, cx)?;
        if !host.truthy(&test) {
            break;
        }
    }
    Ok(Value::Unit)
}
