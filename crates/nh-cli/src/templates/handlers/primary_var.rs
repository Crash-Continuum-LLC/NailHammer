//! Handler for `primary_var`.
//!
//! From `| name:IDENT -> var place`

use nh_runtime::{Ctx, Result};
{{name_import}}

use crate::{Interp, Value};

pub fn run(host: &mut Interp, name: {{name_ty}}, cx: &mut Ctx) -> Result<Value> {
    // What an undeclared name means is a property of the *language*, not of
    // this handler, so it lives on the host next to the symbol table. The
    // braced style calls it an error; the line-oriented style reads it as zero,
    // which is what BASIC has always done.
    host.read(name{{key}}, cx)
}
