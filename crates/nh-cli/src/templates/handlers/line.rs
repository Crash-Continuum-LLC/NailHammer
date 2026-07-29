//! Handler for `line` — one statement and the newline that ends it.
//!
//! Only the line-oriented style has this: with `--style c` a statement ends at
//! `;` and no wrapper is needed. It exists so that a block can be `line*`,
//! which is what lets `WEND` and `END IF` close a body without being listed as
//! part of it.

use nh_runtime::{Ctx, Result};

use crate::{Interp, Value};

pub fn run(_host: &mut Interp, body: Value, _cx: &mut Ctx) -> Result<Value> {
    Ok(body)
}
