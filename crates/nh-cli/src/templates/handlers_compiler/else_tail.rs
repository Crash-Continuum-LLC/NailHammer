use std::rc::Rc;
use nh_runtime::{Ctx, Result};
use crate::generated::ast::Block;
use crate::generated::dispatch::Eval;
use crate::{Interp, Reg};

pub fn run(host: &mut Interp, body: &Rc<Block>, cx: &mut Ctx) -> Result<Reg> {
    body.eval(host, cx)
}
