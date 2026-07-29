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

    /// Reads a name, or says why it cannot.
    ///
    /// **This is a language decision, and it is yours.** Erroring is right for
    /// a language where declaring is deliberate. A language where every name
    /// starts at zero would return `Value::Num(0.0)` here instead — see the
    /// line-oriented scaffold, which does exactly that.
    pub fn read(&self, name: &str, cx: &mut Ctx) -> nh_runtime::Result<Value> {
        match self.get(name) {
            Some(v) => Ok(v.clone()),
            None => cx.err(format!("undefined variable `{name}`")),
        }
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
                self.set(name, value.clone());
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
                .get(name)
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
{{asyncsupport}}
crate::nh_handlers!(Interp);
