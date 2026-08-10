//! `name(args)`
//!
//! The arguments arrive evaluated. The function is looked up **by name, at the
//! moment of the call** — so redefining one takes effect immediately, and
//! calling one before its `fn` statement has run is an error. Pebble runs
//! definitions in order rather than hoisting them; collecting them in a pass
//! before evaluating would be a handful of lines and a different language.

use nh_runtime::{Ctx, Result};
use crate::{Interp, Value};

pub fn run(host: &mut Interp, name: &str, args: Option<Value>, cx: &mut Ctx) -> Result<Value> {
    let Some(f) = host.function(name) else {
        return cx.err(format!("`{name}` is not a function"));
    };
    let args = match args {
        Some(Value::List(items)) => items,
        Some(one) => vec![one],
        None => Vec::new(),
    };
    if args.len() != f.params.len() {
        return cx.err(format!(
            "`{name}` takes {} argument(s), got {}",
            f.params.len(),
            args.len()
        ));
    }
    host.call(&f, args, cx)
}
