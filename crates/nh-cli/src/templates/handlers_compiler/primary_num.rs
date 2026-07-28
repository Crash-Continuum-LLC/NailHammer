//! Handler for `primary_num`.
//!
//! From `= digits:NUMBER -> num`
//!
//! A token binding arrives as text. Parsing it is a build-time job even for a
//! compiler — the literal is known now, so bake it in.

use nh_runtime::{Ctx, Result};

use crate::Interp;

pub fn run(host: &mut Interp, digits: &str, cx: &mut Ctx) -> Result<()> {
    match digits.parse::<f64>() {
        Ok(n) => {
            host.emit_push(n);
            Ok(())
        }
        Err(_) => cx.err("not a valid number"),
    }
}
