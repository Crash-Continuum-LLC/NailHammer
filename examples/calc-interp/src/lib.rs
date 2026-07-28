//! An expression interpreter built on the generated operator driver.
//!
//! The grammar declares **no operator handling at all** — `use operators::core`
//! supplies `expr`, precedence, associativity, and short-circuiting. This crate
//! implements only the `Operators` roles its language actually has; everything
//! else stays at its defaulted "unsupported" error.

use std::collections::HashMap;

pub mod generated;
pub mod handlers;

#[derive(pest_derive::Parser)]
#[grammar = "calc.pest"]
pub struct CalcParser;

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
    /// Indexable slots, so the grammar has a place with an *expression* in it.
    pub slots: HashMap<String, Vec<Value>>,
    /// Every `trace(x)` that actually ran.
    ///
    /// This exists so a test can prove short-circuiting by **observation**
    /// rather than by asserting on intent: if `false && trace(1)` evaluated its
    /// right operand, the effect would show up here.
    pub traced: Vec<f64>,
    pub output: Vec<String>,
}

impl generated::dispatch::Semantics for Interp {
    type Out = Value;
}

impl generated::dispatch::Values for Interp {

    fn truthy(&self, value: &Value) -> bool {
        match value {
            Value::Bool(b) => *b,
            Value::Num(n) => *n != 0.0,
        }
    }
}

impl Interp {
    fn nums(&mut self, lhs: &Value, rhs: &Value, op: &str) -> nh_runtime::Result<(f64, f64)> {
        match (lhs, rhs) {
            (Value::Num(a), Value::Num(b)) => Ok((*a, *b)),
            _ => Err(nh_runtime::Error::runtime(format!(
                "`{op}` needs numbers, got {lhs} and {rhs}"
            ))),
        }
    }
}

// Only the roles this language has. `assign`, `rem`, and the rest keep their
// defaulted errors, and nothing had to be written to decline them.
impl generated::dispatch::Operators for Interp {
    // The standard short-circuit bodies for `&&`, `||` and friends. They live
    // in a macro rather than in trait defaults because they need `Values`, and
    // a bytecode emitter has no values to inspect — it compiles these to jumps
    // instead and writes its own.
    crate::nh_value_operators!();

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
    fn pow(&mut self, lhs: Value, rhs: Value) -> nh_runtime::Result<Value> {
        let (a, b) = self.nums(&lhs, &rhs, "**")?;
        Ok(Value::Num(a.powf(b)))
    }
    fn neg(&mut self, operand: Value) -> nh_runtime::Result<Value> {
        match operand {
            Value::Num(n) => Ok(Value::Num(-n)),
            other => Err(nh_runtime::Error::runtime(format!("cannot negate {other}"))),
        }
    }
    fn not(&mut self, operand: Value) -> nh_runtime::Result<Value> {
        let t = <Self as generated::dispatch::Values>::truthy(self, &operand);
        Ok(Value::Bool(!t))
    }

    /// One method covers the whole comparison tier, because the grammar binds
    /// them all to `-> compare` and the driver hands over a discriminant.
    fn compare(
        &mut self,
        lhs: Value,
        op: generated::dispatch::CompareOp,
        rhs: Value,
    ) -> nh_runtime::Result<Value> {
        use generated::dispatch::CompareOp as C;
        if let (C::EqEq, _) | (C::BangEq, _) = (op, ()) {
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
    // `and_then` and `or_else` are NOT implemented here. Their generated
    // defaults short-circuit correctly using `truthy`, which is the only thing
    // this language had to supply.

    /// Stores a value at a place.
    ///
    /// The place arrives with its parts already evaluated: for `a[i] = v`, `i`
    /// was computed once, before this ran.
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
            Place::PrimaryElem { name, index, .. } => {
                let i = slot_index(&index)?;
                let slots = self.slots.entry(name.to_string()).or_default();
                if slots.len() <= i {
                    slots.resize(i + 1, Value::Num(0.0));
                }
                slots[i] = value.clone();
                Ok(value)
            }
        }
    }

    /// Reads the current value at a place, for compound assignment.
    ///
    /// `compound_assign` is not implemented here at all — its generated default
    /// reads, applies `add`, and writes, and it is correct precisely because the
    /// place was resolved once before either half ran.
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
                .ok_or_else(|| nh_runtime::Error::runtime(format!(
                    "undefined variable `{name}`"
                ))),
            Place::PrimaryElem { name, index, .. } => {
                let i = slot_index(index)?;
                Ok(self
                    .slots
                    .get(*name)
                    .and_then(|s| s.get(i))
                    .cloned()
                    .unwrap_or(Value::Num(0.0)))
            }
        }
    }
}

fn slot_index(value: &Value) -> nh_runtime::Result<usize> {
    match value {
        Value::Num(n) if *n >= 0.0 && n.fract() == 0.0 => Ok(*n as usize),
        other => Err(nh_runtime::Error::runtime(format!(
            "`{other}` is not a valid slot index"
        ))),
    }
}

crate::nh_handlers!(Interp);
