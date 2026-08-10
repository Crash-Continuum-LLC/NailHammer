//! Handler for `primary_str`.
//!
//! From `| text:STRING -> str`

use nh_runtime::{Ctx, Result};

use crate::{Interp, Value};

pub fn run(_host: &mut Interp, text: &str, _cx: &mut Ctx) -> Result<Value> {
    // The token includes its quotes.
    Ok(Value::Str(text[1..text.len() - 1].to_string()))
}
