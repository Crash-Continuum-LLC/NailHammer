//! {{Name}} — a language built with NailHammer.
//!
//! Read `src/handlers/` first: one small file per grammar alternative, each
//! reading its inputs **by name**. There is no `into_inner()` anywhere and no
//! child is addressed by position.
//!
//! What is generated (from `{{name}}.nh`, by `nh build --rust src`):
//!   * `src/{{name}}.pest`   — the parser grammar
//!   * `src/generated/**`    — the AST and its builder, the trait stack,
//!                             evaluation, diagnostics
//!
//! What is yours: this file, `src/main.rs`, and `src/handlers/*.rs`.

use std::collections::HashMap;

use nh_runtime::Ctx;
{{name_import}}

pub mod generated;
pub mod handlers;

#[derive(pest_derive::Parser)]
#[grammar = "{{name}}.pest"]
pub struct {{Name}}Parser;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Num(f64),
    Bool(bool),
    /// What a statement, an empty block, or a loop evaluates to.
    Unit,
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Num(n) => write!(f, "{n}"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Unit => Ok(()),
        }
    }
}

{{host_types}}#[derive(Debug, Default)]
pub struct Interp {
    /// The global scope. A function call pushes a frame onto `locals`; a name
    /// is looked up there first and here second.
    pub vars: HashMap<String, Value>,
    /// One frame per call in progress. Empty until something calls something.
    ///
    /// It is here even when the scaffold has no functions so that `get` and
    /// `set` are written once — adding functions later then changes no handler.
    pub locals: Vec<HashMap<String, Value>>,
    pub output: Vec<String>,
{{host_state}}}

impl generated::dispatch::Semantics for Interp {
    type Out = Value;
}

impl generated::dispatch::Values for Interp {

    /// Used by the short-circuit operator defaults. Supplying this is the only
    /// thing `&&` and `||` need from you.
    fn truthy(&self, value: &Value) -> bool {
        match value {
            Value::Bool(b) => *b,
            Value::Num(n) => *n != 0.0,
            Value::Unit => false,
        }
    }
}

impl Interp {
    /// Reads a name: the innermost call frame first, then the globals.
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.locals
            .last()
            .and_then(|frame| frame.get(name))
            .or_else(|| self.vars.get(name))
    }

    /// Reads a name. An undeclared one is zero.
    ///
    /// **This is a language decision, and it is yours.** BASIC has always
    /// started every variable at zero, so `PRINT total` before any `LET total`
    /// prints `0` rather than failing. Return an error here instead and
    /// declaring becomes deliberate — which is what the braced scaffold does.
    pub fn read(&self, name: &str, _cx: &mut Ctx) -> nh_runtime::Result<Value> {
        Ok(self.get(name).cloned().unwrap_or(Value::Num(0.0)))
    }

    /// Writes a name, into the innermost frame if there is one.
    pub fn set(&mut self, name: &str, value: Value) {
        match self.locals.last_mut() {
            Some(frame) => frame.insert(name.to_string(), value),
            None => self.vars.insert(name.to_string(), value),
        };
    }
{{host_impl}}
    fn nums(&self, lhs: &Value, rhs: &Value, op: &str) -> nh_runtime::Result<(f64, f64)> {
        match (lhs, rhs) {
            (Value::Num(a), Value::Num(b)) => Ok((*a, *b)),
            _ => Err(nh_runtime::Error::runtime(format!(
                "`{op}` needs numbers, got {lhs} and {rhs}"
            ))),
        }
    }
}

/// Operator semantics.
///
/// Every method is defaulted to an "unsupported" error, so you implement only
/// what your language has. `%`, `&&`, and `||` are all in `operators::core` and
/// all work — `%` will report itself honestly if used, and the short-circuit
/// operators already behave correctly from `truthy` alone.
impl generated::dispatch::Operators for Interp {
    fn add(&mut self, lhs: Value, rhs: Value) -> nh_runtime::Result<Value> {
        let (a, b) = self.nums(&lhs, &rhs, "+")?;
        Ok(Value::Num(a + b))
    }
    fn sub(&mut self, lhs: Value, rhs: Value) -> nh_runtime::Result<Value> {
        let (a, b) = self.nums(&lhs, &rhs, "-")?;
        Ok(Value::Num(a - b))
    }
    fn mul(&mut self, lhs: Value, rhs: Value) -> nh_runtime::Result<Value> {
        let (a, b) = self.nums(&lhs, &rhs, "*")?;
        Ok(Value::Num(a * b))
    }
    fn div(&mut self, lhs: Value, rhs: Value) -> nh_runtime::Result<Value> {
        let (a, b) = self.nums(&lhs, &rhs, "/")?;
        if b == 0.0 {
            return Err(nh_runtime::Error::runtime("division by zero"));
        }
        Ok(Value::Num(a / b))
    }
    fn neg(&mut self, operand: Value) -> nh_runtime::Result<Value> {
        match operand {
            Value::Num(n) => Ok(Value::Num(-n)),
            other => Err(nh_runtime::Error::runtime(format!("cannot negate {other}"))),
        }
    }

    /// `MOD`, from `left word "MOD" -> rem;` in the grammar. The method is
    /// named for the *role*, never for the spelling — rename the word and this
    /// does not move.
    fn rem(&mut self, lhs: Value, rhs: Value) -> nh_runtime::Result<Value> {
        let (a, b) = self.nums(&lhs, &rhs, "MOD")?;
        if b == 0.0 {
            return Err(nh_runtime::Error::runtime("MOD by zero"));
        }
        Ok(Value::Num(a % b))
    }

    /// `NOT`, which in this style is a word rather than a symbol. `word "NOT"`
    /// in the table guards the identifier boundary for you, so `NOTE` is still
    /// a variable.
    fn not(&mut self, operand: Value) -> nh_runtime::Result<Value> {
        let t = <Self as generated::dispatch::Values>::truthy(self, &operand);
        Ok(Value::Bool(!t))
    }

    /// One method covers the whole comparison tier, because the table binds
    /// them all to one role and hands over a discriminant.
    fn compare(
        &mut self,
        lhs: Value,
        op: generated::dispatch::CompareOp,
        rhs: Value,
    ) -> nh_runtime::Result<Value> {
        use generated::dispatch::CompareOp as C;
        // `=` is equality here. Assignment is a statement (`LET x = 1`), which
        // is exactly why BASIC has the `LET` keyword.
        if matches!(op, C::Eq | C::LtGt) {
            let equal = lhs == rhs;
            return Ok(Value::Bool(if op == C::Eq { equal } else { !equal }));
        }
        let (a, b) = self.nums(&lhs, &rhs, op.spelling())?;
        Ok(Value::Bool(match op {
            C::Lt => a < b,
            C::LtEq => a <= b,
            C::Gt => a > b,
            C::GtEq => a >= b,
            C::Eq | C::LtGt => unreachable!("handled above"),
        }))
    }

}

// Writes the `Handlers` impl: one delegating method per grammar alternative.
// Add an alternative to `{{name}}.nh` and this stops compiling until a handler
// exists — Rust's own trait exhaustiveness does the checking.
{{asyncsupport}}
crate::nh_handlers!(Interp);
