//! Regenerates the parser, views, dispatch — and the operator implementation.
//!
//! `.target("nh-vm")` is the only line that differs from an ordinary NailHammer
//! project, and it is what makes `src/generated/vm_operators.rs` appear. No
//! handler in this crate implements an operator.
fn main() {
    nh_build::Builder::new("lang.nh").target("nh-vm").run();
}
