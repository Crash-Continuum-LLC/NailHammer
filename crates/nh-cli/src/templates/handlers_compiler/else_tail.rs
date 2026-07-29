//! Handler for `else_tail` — the `else` half of an `if`.
//!
//! Reached only when `stmt_iff` decides where this branch goes, because the
//! whole `otherwise` binding is `lazy`.

use std::rc::Rc;

use nh_runtime::{Ctx, Result};

use crate::generated::ast::Block;
use crate::generated::dispatch::Eval;
use crate::Interp;

pub fn run(host: &mut Interp, body: &Rc<Block>, cx: &mut Ctx) -> Result<()> {
    body.eval(host, cx)
}
