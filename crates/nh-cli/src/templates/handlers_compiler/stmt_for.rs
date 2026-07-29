//! A counting loop.
//!
//! Nothing here knows whether the counter is a slot or a global — `read_var`
//! and `emit_increment` answer that, and the difference is three instructions
//! per iteration against one.

use nh_runtime::Shared;
use nh_runtime::{Ctx, Result};
{{name_import}}use crate::generated::ast::Block;
use crate::generated::dispatch::Eval;
use crate::{Interp, Reg};

pub fn run(
    host: &mut Interp,
    var: {{name_ty}},
    from: Reg,
    to: Reg,
    body: &Shared<Block>,
    cx: &mut Ctx,
) -> Result<Reg> {
    let home = host.write_var(var{{key}}, from);
    host.free(home);

    let top = host.here();
    let cur = host.read_var(var{{key}});
    let test = host.compare_le(cur, to);
    let to_end = host.emit_jump_if_false(test);
    host.free(test);

    host.enter_loop();
    body.eval(host, cx)?;

    // Where `continue` lands. Skipping the increment would never terminate.
    let step = host.here();
    host.emit_increment(var{{key}});
    host.emit_jump_to(top);

    host.patch_to_here(to_end);
    host.exit_loop(step);
    host.free(to);
    Ok(host.next_reg())
}
