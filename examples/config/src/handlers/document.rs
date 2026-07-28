//! Handler for `document`.
//!
//! From `rule document = SOI entries:entry* EOI -> doc;`

use nh_runtime::{Ctx, Result};

use crate::{Interp, Value};

/// `entries` is a `Vec` because the grammar says `entry*`, and the entries are
/// already evaluated because `entry` is a rule. Change the `*` to a `?` and
/// this stops compiling — which is the point.
pub fn run(_host: &mut Interp, entries: Vec<Value>, cx: &mut Ctx) -> Result<Value> {
    let mut fields = Vec::new();
    for entry in entries {
        match entry.into_field() {
            Some(pair) => fields.push(pair),
            None => return cx.err("expected a key/value entry"),
        }
    }
    Ok(Value::Table(fields))
}
