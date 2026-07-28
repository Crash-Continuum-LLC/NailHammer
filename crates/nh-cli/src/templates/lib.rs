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

pub mod generated;
pub mod handlers;

#[derive(pest_derive::Parser)]
#[grammar = "{{name}}.pest"]
pub struct {{Name}}Parser;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Num(f64),
    Bool(bool),
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Num(n) => write!(f, "{n}"),
            Value::Bool(b) => write!(f, "{b}"),
        }
    }
}

#[derive(Debug, Default)]
pub struct Interp {
    pub vars: HashMap<String, Value>,
    pub output: Vec<String>,
}

impl generated::dispatch::Semantics for Interp {
    type Out = Value;

    /// Used by the short-circuit operator defaults. Supplying this is the only
    /// thing `&&` and `||` need from you.
    fn truthy(&self, value: &Value) -> bool {
        match value {
            Value::Bool(b) => *b,
            Value::Num(n) => *n != 0.0,
        }
    }
}

impl Interp {
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

    /// One method covers the whole comparison tier, because the table binds
    /// them all to one role and hands over a discriminant.
    fn compare(
        &mut self,
        lhs: Value,
        op: generated::dispatch::CompareOp,
        rhs: Value,
    ) -> nh_runtime::Result<Value> {
        use generated::dispatch::CompareOp as C;
        if matches!(op, C::EqEq | C::BangEq) {
            let equal = lhs == rhs;
            return Ok(Value::Bool(if op == C::EqEq { equal } else { !equal }));
        }
        let (a, b) = self.nums(&lhs, &rhs, op.spelling())?;
        Ok(Value::Bool(match op {
            C::Lt => a < b,
            C::LtEq => a <= b,
            C::Gt => a > b,
            C::GtEq => a >= b,
            C::EqEq | C::BangEq => unreachable!("handled above"),
        }))
    }

    /// Stores a value at a place. The place arrives with its parts already
    /// evaluated, exactly once.
    fn assign(
        &mut self,
        place: generated::place::Place<'_, Value>,
        value: Value,
    ) -> nh_runtime::Result<Value> {
        use generated::place::Place;
        match place {
            Place::PrimaryVar { name, .. } => {
                self.vars.insert(name.to_string(), value.clone());
                Ok(value)
            }
        }
    }

    /// Reads the current value at a place, for compound assignment. Add
    /// `right "+=" below "=" -> assign;` to the grammar and `+=` works with no
    /// further code — its default is written in terms of these two methods.
    fn place_read(
        &mut self,
        place: &generated::place::Place<'_, Value>,
    ) -> nh_runtime::Result<Value> {
        use generated::place::Place;
        match place {
            Place::PrimaryVar { name, .. } => self
                .vars
                .get(*name)
                .cloned()
                .ok_or_else(|| {
                    nh_runtime::Error::runtime(format!("undefined variable `{name}`"))
                }),
        }
    }
}

// Writes the `Handlers` impl: one delegating method per grammar alternative.
// Add an alternative to `{{name}}.nh` and this stops compiling until a handler
// exists — Rust's own trait exhaustiveness does the checking.
crate::nh_handlers!(Interp);
