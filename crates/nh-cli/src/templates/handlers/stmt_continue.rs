//! Handler for `stmt_continue`.
//!
//! See `stmt_break.rs` — the same mechanism with a different label. The loop
//! handlers decide what each one means; nothing between here and there knows.

use nh_runtime::{Ctx, Result};

use crate::{Interp, Value};

pub fn run(_host: &mut Interp, cx: &mut Ctx) -> Result<Value> {
    Err(cx.signal("continue"))
}
