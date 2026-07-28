//! Handler for `stmt_let` — `LET x = 1`, or just `x = 1`.
//!
//! From `| "LET"? target:IDENT "=" value:expr -> let`

use nh_runtime::{Ctx, Name, Result};

use crate::{Interp, Value};

pub fn run(host: &mut Interp, target: &Name, value: Value, _cx: &mut Ctx) -> Result<Value> {
    // `.key()`, not `.text()`: the token folds case, so `Count` and `COUNT`
    // must land in the same slot. There is no `&str` here to reach for by
    // mistake — the type makes the folding hard to skip.
    host.store(target.key().to_string(), value);
    Ok(Value::Nothing)
}
