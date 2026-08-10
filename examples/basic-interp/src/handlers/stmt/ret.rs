//! Handler for `stmt_ret` — `RETURN expr`.
//!
//! From `| "RETURN" value:expr -> ret`
//!
//! The value rides on the interpreter and the signal is just the word, for the
//! same reason `GOTO` carries its line number that way: `Error::Signal` cannot
//! know what a BASIC value is.

use nh_runtime::{Ctx, Result};

use crate::{Interp, Value};

pub fn run(host: &mut Interp, value: Value, cx: &mut Ctx) -> Result<Value> {
    host.ret = Some(value);
    Err(cx.signal("RETURN"))
}
