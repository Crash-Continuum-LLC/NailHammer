//! A bare name → whatever it was bound to.
use nh_runtime::{Ctx, Result};
use crate::{Interp, Value};

pub fn run(host: &mut Interp, name: &str, cx: &mut Ctx) -> Result<Value> {
    match host.get(name) {
        Some(v) => Ok(v.clone()),
        None => cx.err(format!("`{name}` is not defined")),
    }
}
