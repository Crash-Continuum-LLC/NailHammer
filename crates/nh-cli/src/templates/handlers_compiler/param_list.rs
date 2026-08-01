//! Never evaluated: `params` is `lazy`, so `stmt_fn` reads it for names.
use nh_runtime::{Ctx, Result};
{{name_import}}use crate::{Interp, Reg};

pub fn run(_host: &mut Interp, _first: {{name_ty}}, _rest: Vec<Reg>, _cx: &mut Ctx) -> Result<Reg> {
    unreachable!("a parameter list is read for its names, never evaluated")
}
