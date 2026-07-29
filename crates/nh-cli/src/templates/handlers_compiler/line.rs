//! One statement and the newline that ends it. Nothing to emit: the statement
//! already did.

use nh_runtime::{Ctx, Result};
use crate::{Interp, Reg};

pub fn run(host: &mut Interp, _body: Reg, _cx: &mut Ctx) -> Result<Reg> {
    Ok(host.next_reg())
}
