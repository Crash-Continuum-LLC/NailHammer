//! A worked NailHammer interpreter.
//!
//! This exists to validate DESIGN.md's central claim, so read `src/handlers/`
//! rather than this file. Each handler is a few lines, lives in its own file,
//! and reads its inputs **by name**. There is no `into_inner()` anywhere, and
//! no child is ever addressed by position.
//!
//! What is hand-written: `src/handlers/*.rs`, this file, and `value.rs`.
//! What is generated: everything in `src/generated/` and `src/config.pest`,
//! from `config.nh` via `nh build config.nh -o src/config.pest --rust src`.




pub mod generated;
pub mod handlers;
mod value;

pub use value::Value;

#[derive(pest_derive::Parser)]
#[grammar = "config.pest"]
pub struct ConfigParser;

/// The interpreter.
///
/// It carries no state, which is the point: everything a handler needs arrives
/// through its view or through `cx`.
pub struct Interp;

impl generated::dispatch::Semantics for Interp {
    type Out = Value;
}

impl generated::dispatch::Values for Interp {

    fn truthy(&self, value: &Value) -> bool {
        !matches!(value, Value::Bool(false) | Value::Null)
    }
}

// This grammar declares no operators, so nothing needs overriding. A language
// with arithmetic would implement `add`, `mul`, and friends here and get the
// whole table's parsing and precedence for free.
impl generated::dispatch::Operators for Interp {}

// Writes the `Handlers` impl: one delegating method per grammar alternative,
// each calling into `handlers::<name>::run`. Add an alternative to `config.nh`
// and this stops compiling until a handler exists.
crate::nh_handlers!(Interp);
