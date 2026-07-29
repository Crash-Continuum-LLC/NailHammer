use nh_runtime::Shared;
use nh_runtime::{Ctx, Result};
use crate::generated::ast::Block;
use crate::generated::dispatch::Eval;
use crate::{Interp, Reg};

pub fn run(host: &mut Interp, body: &Shared<Block>, cx: &mut Ctx) -> Result<Reg> {
    body.eval(host, cx)
}
