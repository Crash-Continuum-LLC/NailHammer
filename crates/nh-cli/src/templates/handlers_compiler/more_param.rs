//! Handler for `more_param` — one `, name` in a parameter list.
//!
//! Never evaluated. `params` is bound `lazy` in the grammar because a
//! definition wants the parameter *names* — evaluating them would look them up
//! as variables that do not exist yet — so `stmt_fn` walks this node instead.
//! The trait still requires a handler, and being honest about it is better
//! than a body that pretends.

use nh_runtime::{Ctx, Result};
{{name_import}}

use crate::Interp;

pub fn run(_host: &mut Interp, _name: {{name_ty}}, _cx: &mut Ctx) -> Result<()> {
    unreachable!("a parameter list is read for its names, never evaluated")
}
