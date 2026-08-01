//! `value:NUMBER -> num`
use nh_runtime::{Ctx, Result};
use nh_vm::{Reg, Value};

use nh_vm::Emitter;

use crate::Interp;

pub fn run(host: &mut Interp, value: &str, cx: &mut Ctx) -> Result<Reg> {
    match value.parse::<f64>() {
        Ok(n) => Ok(host.konst(Value::Num(n))),
        Err(_) => cx.err("not a number this machine can hold"),
    }
}
