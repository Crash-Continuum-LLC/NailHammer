//! Handler for `else_tail` — the `else` half of an `if`.
//!
//! Reached only when `stmt_iff` chooses to evaluate it, because the whole
//! `otherwise` binding is `lazy`.

use nh_runtime::Shared;

use nh_runtime::{Ctx, Result};

use crate::generated::ast::Block;
use crate::generated::dispatch::Eval;
use crate::{Interp, Value};

pub fn run(host: &mut Interp, body: &Shared<Block>, cx: &mut Ctx) -> Result<Value> {
    body.eval(host, cx)
}
