//! Pebble — the language built in the guide.

pub mod generated;
pub mod handlers;

/// The parser pest derives from the generated grammar.
#[derive(pest_derive::Parser)]
#[grammar = "pebble.pest"]
pub struct PebbleParser;

use std::collections::HashMap;
use nh_runtime::Result;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Num(f64),
    Text(String),
    Bool(bool),
    Point(f64, f64),
    /// Only ever an argument list on its way into a call.
    List(Vec<Value>),
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Null => write!(f, "null"),
            Value::Num(n) => write!(f, "{n}"),
            Value::Text(s) => write!(f, "{s}"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Point(x, y) => write!(f, "({x}, {y})"),
            Value::List(items) => {
                let parts: Vec<String> = items.iter().map(|v| v.to_string()).collect();
                write!(f, "[{}]", parts.join(", "))
            }
        }
    }
}

/// A function: its parameter names and its body, kept for later.
///
/// The body is `Shared<Block>` rather than a copy — `lazy` handed the handler
/// an owned node, so storing it costs a refcount.
#[derive(Clone)]
pub struct Function {
    pub params: Vec<String>,
    pub body: nh_runtime::Shared<generated::ast::Block>,
}

/// How deep calls may nest before Pebble gives up.
///
/// Recursion without a base case is a program bug, and the honest response is
/// a diagnostic — not a stack overflow that takes the process down and cannot
/// be caught.
///
/// The number is bounded by the *host* stack, not by taste: each Pebble call
/// costs several Rust frames, and a spawned thread gets far less stack than
/// `main` does. 256 passed from the command line and aborted the test runner,
/// which is exactly the sort of thing a test suite is for.
const MAX_DEPTH: usize = 128;

#[derive(Default)]
pub struct Interp {
    /// Globals. Present even inside a call, since a function can read them.
    vars: HashMap<String, Value>,
    /// One map per active call. A parameter lives here, so a recursive call
    /// cannot write over its caller's copy.
    frames: Vec<HashMap<String, Value>>,
    funcs: HashMap<String, Function>,
    /// Where a `return` leaves its value on the way out.
    returned: Option<Value>,
    pub output: Vec<String>,
}

impl Interp {
    /// Writes to the innermost frame that already holds the name, else to the
    /// current scope — so `n = n + 1` inside a function updates the local `n`
    /// rather than shadowing it, and a name first assigned in a function stays
    /// local to it.
    pub fn set(&mut self, name: &str, v: Value) {
        if let Some(frame) = self.frames.last_mut() {
            frame.insert(name.to_string(), v);
            return;
        }
        self.vars.insert(name.to_string(), v);
    }

    /// Innermost frame first, then globals.
    pub fn get(&self, name: &str) -> Option<&Value> {
        if let Some(frame) = self.frames.last() {
            if let Some(v) = frame.get(name) {
                return Some(v);
            }
        }
        self.vars.get(name)
    }

    pub fn define(&mut self, name: &str, f: Function) {
        self.funcs.insert(name.to_string(), f);
    }

    pub fn function(&self, name: &str) -> Option<Function> {
        self.funcs.get(name).cloned()
    }

    /// Runs a function body in a frame of its own.
    pub fn call(
        &mut self,
        f: &Function,
        args: Vec<Value>,
        cx: &mut nh_runtime::Ctx,
    ) -> Result<Value> {
        if self.frames.len() >= MAX_DEPTH {
            return Err(nh_runtime::Error::runtime(format!(
                "call nested more than {MAX_DEPTH} deep — is the recursion missing a base case?"
            )));
        }
        let frame = f.params.iter().cloned().zip(args).collect();
        self.frames.push(frame);

        use crate::generated::dispatch::Eval;
        let outcome = f.body.eval(self, cx);

        self.frames.pop();
        match outcome {
            // The signal `return` raises: not a failure, an early exit.
            Err(e) if e.is_signal("return") => Ok(self.returned.take().unwrap_or(Value::Null)),
            Err(e) => Err(e),
            // Falling off the end of a body yields null, not the last value —
            // a function that means to give something back says so.
            Ok(_) => Ok(Value::Null),
        }
    }

    pub fn stash_return(&mut self, v: Value) {
        self.returned = Some(v);
    }
    /// Pebble's one opinion about truth, in the one place that has it.
    pub fn is_true(&self, v: &Value) -> bool {
        // A point is a thing, even at the origin -- `(0, 0)` is somewhere.
        !matches!(v, Value::Null | Value::Bool(false) | Value::Num(0.0))
    }
    fn num(&self, v: &Value) -> Result<f64> {
        match v {
            Value::Num(n) => Ok(*n),
            other => Err(nh_runtime::Error::runtime(format!(
                "expected a number, got `{other}`"
            ))),
        }
    }
}

impl generated::dispatch::Semantics for Interp {
    type Out = Value;
}

impl generated::dispatch::Values for Interp {
    fn truthy(&self, v: &Value) -> bool {
        Interp::is_true(self, v)
    }
    fn is_null(&self, v: &Value) -> bool {
        matches!(v, Value::Null)
    }
}

impl generated::dispatch::Operators for Interp {
    fn add(&mut self, l: Value, r: Value) -> Result<Value> {
        // `+` concatenates when either side is text: the one place Pebble
        // makes a language decision inside an operator.
        if let (Value::Text(a), b) = (&l, &r) {
            return Ok(Value::Text(format!("{a}{b}")));
        }
        if let (a, Value::Text(b)) = (&l, &r) {
            return Ok(Value::Text(format!("{a}{b}")));
        }
        // Points add componentwise. Nothing forced this arm; `add` is not an
        // exhaustive match, so this is a decision to make rather than an error
        // to fix.
        if let (Value::Point(ax, ay), Value::Point(bx, by)) = (&l, &r) {
            return Ok(Value::Point(ax + bx, ay + by));
        }
        let (a, b) = (self.num(&l)?, self.num(&r)?);
        Ok(Value::Num(a + b))
    }
    fn sub(&mut self, l: Value, r: Value) -> Result<Value> {
        let (a, b) = (self.num(&l)?, self.num(&r)?);
        Ok(Value::Num(a - b))
    }
    fn mul(&mut self, l: Value, r: Value) -> Result<Value> {
        let (a, b) = (self.num(&l)?, self.num(&r)?);
        Ok(Value::Num(a * b))
    }
    fn div(&mut self, l: Value, r: Value) -> Result<Value> {
        let (a, b) = (self.num(&l)?, self.num(&r)?);
        if b == 0.0 {
            return Err(nh_runtime::Error::runtime("division by zero".to_string()));
        }
        Ok(Value::Num(a / b))
    }
    fn rem(&mut self, l: Value, r: Value) -> Result<Value> {
        let (a, b) = (self.num(&l)?, self.num(&r)?);
        Ok(Value::Num(a % b))
    }
    fn neg(&mut self, v: Value) -> Result<Value> {
        let n = self.num(&v)?;
        Ok(Value::Num(-n))
    }
    fn not(&mut self, v: Value) -> Result<Value> {
        let t = <Self as generated::dispatch::Values>::truthy(self, &v);
        Ok(Value::Bool(!t))
    }
    fn compare(
        &mut self,
        l: Value,
        op: generated::dispatch::CompareOp,
        r: Value,
    ) -> Result<Value> {
        use generated::dispatch::CompareOp::*;
        let ord = match (&l, &r) {
            (Value::Num(a), Value::Num(b)) => a.partial_cmp(b),
            (Value::Text(a), Value::Text(b)) => Some(a.cmp(b)),
            _ => None,
        };
        Ok(Value::Bool(match op {
            EqEq => l == r,
            BangEq => l != r,
            _ => match ord {
                Some(o) => match op {
                    Lt => o.is_lt(),
                    LtEq => o.is_le(),
                    Gt => o.is_gt(),
                    GtEq => o.is_ge(),
                    _ => unreachable!("== and != handled above"),
                },
                None => false,
            },
        }))
    }
    // `Place` is generated from the grammar: one variant per alternative
    // marked `place`. Pebble marks exactly one, so this match has one arm and
    // adding a second assignable form would stop it compiling.
    fn assign(
        &mut self,
        p: generated::place::Place<'_, Value>,
        r: Value,
    ) -> Result<Value> {
        let generated::place::Place::AtomName { name, .. } = p;
        self.set(name, r.clone());
        Ok(r)
    }
    fn place_read(
        &mut self,
        p: &generated::place::Place<'_, Value>,
    ) -> Result<Value> {
        let generated::place::Place::AtomName { name, .. } = p;
        match self.get(name) {
            Some(v) => Ok(v.clone()),
            None => Err(nh_runtime::Error::runtime(format!(
                "`{name}` is not defined"
            ))),
        }
    }
}

crate::nh_handlers!(Interp);
