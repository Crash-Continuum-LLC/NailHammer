//! An argument list.
//!
//! `args:expr ("," args:expr)*` binds the same name on both sides of the
//! separator, so every element arrives in one `Vec` — head included.

use nh_runtime::{Ctx, Result};
use crate::{Interp, Value};

pub fn run(_host: &mut Interp, args: Vec<Value>, _cx: &mut Ctx) -> Result<Value> {
    Ok(Value::List(args))
}
