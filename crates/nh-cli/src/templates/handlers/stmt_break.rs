//! Handler for `stmt_break`.
//!
//! A signal, not an error. `Error::Signal` exists because `?` propagation is
//! already exactly the unwinding a non-local jump needs — the only thing
//! missing was a variant that does not mean "something went wrong".
//!
//! The runtime never interprets the label. It carries it, and if it reaches
//! the top uncaught, reports it by name: "`break` is not inside anything that
//! handles it". That is why the label is a string rather than an opaque tag.

use nh_runtime::{Ctx, Result};

use crate::{Interp, Value};

pub fn run(_host: &mut Interp, cx: &mut Ctx) -> Result<Value> {
    Err(cx.signal("break"))
}
