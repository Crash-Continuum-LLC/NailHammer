//! `TEXT` → a string, with the quotes taken off.
use nh_runtime::{Ctx, Result};
use crate::{Interp, Value};

pub fn run(_host: &mut Interp, text: &str, _cx: &mut Ctx) -> Result<Value> {
    // The token includes its quotes, because the grammar matched them.
    Ok(Value::Text(text[1..text.len() - 1].to_string()))
}
