//! `text:STRING -> str`
//!
//! The token includes its quotes, because the grammar matched them; the
//! constant should not.
use nh_runtime::{Ctx, Result};
use nh_vm::{Op, Reg, Value};

use nh_vm::Emitter;

use crate::Interp;

pub fn run(host: &mut Interp, text: &str, _cx: &mut Ctx) -> Result<Reg> {
    let inner = text.trim_matches('"');
    let dst = host.alloc();
    host.emit(Op::LoadK { dst, value: Value::str(inner) });
    Ok(dst)
}
