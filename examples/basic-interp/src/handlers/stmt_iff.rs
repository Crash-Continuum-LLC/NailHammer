//! Handler for `stmt_iff` — `IF cond THEN stmt`.
//!
//! From `| "IF" cond:expr "THEN" lazy body:stmt -> iff`
//!
//! `body` is `lazy`, so it does not run unless this handler runs it. Without
//! that, `IF n > 0 THEN GOTO 10` would jump unconditionally and never
//! terminate.

use std::rc::Rc;

use nh_runtime::{Ctx, Result};

use crate::generated::ast::Stmt;
use crate::generated::dispatch::{Eval, Semantics};
use crate::{Interp, Value};

pub fn run(host: &mut Interp, cond: Value, body: &Rc<Stmt>, cx: &mut Ctx) -> Result<Value> {
    // `truthy`, not a match on a false literal: the language has one definition
    // of truth and it lives in `Semantics`, shared with `AND`/`OR`/`NOT`.
    if !host.truthy(&cond) {
        return Ok(Value::Nothing);
    }
    body.eval(host, cx)
}
