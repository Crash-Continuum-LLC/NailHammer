//! A mini BASIC interpreter.
//!
//! Two things this example exists to show:
//!
//! * **`lazy` makes loops possible.** A `FOR` body has to run once per
//!   iteration. Handler parameters normally arrive already evaluated, which
//!   would run the body exactly once, before the loop started. `lazy body:stmt*`
//!   hands the handler a `Vec<Deferred>` instead, and the loop forces it.
//! * **Case folding is in the type.** `IDENT` folds case, so every binding to
//!   it arrives as `Ident` rather than `&str`. Looking a variable up without
//!   folding is not something you can forget to do — there is no `&str` to
//!   reach for.

use std::collections::HashMap;
use nh_runtime::Shared;

pub mod generated;
pub mod handlers;

#[derive(pest_derive::Parser)]
#[grammar = "basic.pest"]
pub struct BasicParser;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Num(f64),
    Str(String),
    /// An argument list on its way to a call. BASIC has no list literal, so
    /// this is only ever produced by `arg_list` and consumed by a call.
    List(Vec<Value>),
    /// What a statement evaluates to. BASIC statements are not expressions.
    Nothing,
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Num(n) => write!(f, "{n}"),
            Value::Str(s) => write!(f, "{s}"),
            Value::List(items) => {
                let parts: Vec<String> = items.iter().map(|v| v.to_string()).collect();
                write!(f, "{}", parts.join(", "))
            }
            Value::Nothing => Ok(()),
        }
    }
}

#[derive(Debug, Default)]
pub struct Interp {
    /// Keyed by the **folded** name, so `I` and `i` are one variable.
    pub vars: HashMap<String, Value>,
    pub output: Vec<String>,
    /// Subroutine bodies, kept from `SUB` and run at every later `CALL`.
    ///
    /// **This is the field that could not exist before M7.** A `lazy` binding
    /// used to borrow the parse tree, so a handler could run it but never keep
    /// it; the tree is owned now, so a body is just data (DESIGN.md §9).
    pub subs: HashMap<String, Vec<Shared<generated::ast::Line>>>,
    /// How deep `CALL` is nested, so runaway recursion reports instead of
    /// overflowing the stack.
    depth: usize,
    /// Functions: parameter names and a body, kept the same way `SUB` bodies are.
    pub funcs: HashMap<String, Function>,
    /// One frame per active call, holding that call's parameters.
    ///
    /// `SUB` needed none of this — it takes no arguments, so everything it
    /// touches is global. A function's parameters have to be *local*, or
    /// recursion would have every frame writing over the same `n`.
    pub frames: Vec<HashMap<String, Value>>,
    /// The value a pending `RETURN` is carrying.
    ///
    /// `Error::Signal` deliberately carries no payload — the runtime has no
    /// idea what a BASIC value is — so it rides here, exactly like `GOTO`'s
    /// target line number.
    pub ret: Option<Value>,
    /// Where the pending `GOTO` wants to go.
    ///
    /// A jump carries a value, and `Error::Signal` deliberately does not — the
    /// runtime has no idea what a BASIC line number is. So the target rides
    /// here and the signal is just the word `goto`.
    pub jump: Option<String>,
}

/// Deep enough for any reasonable program, shallow enough to report rather
/// than abort. Recursive evaluation is a real stack, and a stack overflow is
/// not a diagnostic — it kills the process with no location.
const MAX_CALL_DEPTH: usize = 128;

/// A function definition: its parameter names and its body.
#[derive(Clone, Debug)]
pub struct Function {
    pub params: Vec<String>,
    pub body: Vec<nh_runtime::Shared<generated::ast::Line>>,
}

impl Interp {
    /// Reads a variable: the innermost frame first, then globals.
    pub fn lookup(&self, key: &str) -> Option<&Value> {
        self.frames
            .last()
            .and_then(|f| f.get(key))
            .or_else(|| self.vars.get(key))
    }

    /// Writes a variable. A name bound as a parameter stays local; anything
    /// else is global, which is what BASIC programmers expect.
    pub fn store(&mut self, key: String, value: Value) {
        if let Some(slot) = self.frames.last_mut().and_then(|f| f.get_mut(&key)) {
            *slot = value;
            return;
        }
        self.vars.insert(key, value);
    }

    pub fn enter_call(&mut self) -> nh_runtime::Result<()> {
        self.depth += 1;
        if self.depth > MAX_CALL_DEPTH {
            self.depth -= 1;
            return Err(nh_runtime::Error::runtime(format!(
                "`CALL` nested more than {MAX_CALL_DEPTH} deep; this is probably \
                 infinite recursion"
            )));
        }
        Ok(())
    }

    pub fn leave_call(&mut self) {
        self.depth -= 1;
    }
}

impl generated::dispatch::Semantics for Interp {
    type Out = Value;
}

impl generated::dispatch::Values for Interp {

    /// BASIC's own convention: zero and the empty string are false.
    fn truthy(&self, value: &Value) -> bool {
        match value {
            Value::Num(n) => *n != 0.0,
            Value::Str(s) => !s.is_empty(),
            Value::List(items) => !items.is_empty(),
            Value::Nothing => false,
        }
    }
}

impl Interp {
    fn nums(&self, lhs: &Value, rhs: &Value, op: &str) -> nh_runtime::Result<(f64, f64)> {
        match (lhs, rhs) {
            (Value::Num(a), Value::Num(b)) => Ok((*a, *b)),
            _ => Err(nh_runtime::Error::runtime(format!(
                "`{op}` needs numbers, got `{lhs}` and `{rhs}`"
            ))),
        }
    }

    /// BASIC's truth values: `-1` for true, `0` for false.
    fn flag(b: bool) -> Value {
        Value::Num(if b { -1.0 } else { 0.0 })
    }
}

// Only the roles this language has. Everything else — `assign`, `pow`, the
// short-circuit pair — keeps its defaulted "unsupported" error, and declining
// them took no code.
impl generated::dispatch::Operators for Interp {
    fn add(&mut self, lhs: Value, rhs: Value) -> nh_runtime::Result<Value> {
        // `+` concatenates when either side is a string, which is the one place
        // this language is not arithmetic-only.
        if let (Value::Str(_), _) | (_, Value::Str(_)) = (&lhs, &rhs) {
            return Ok(Value::Str(format!("{lhs}{rhs}")));
        }
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

    fn rem(&mut self, lhs: Value, rhs: Value) -> nh_runtime::Result<Value> {
        let (a, b) = self.nums(&lhs, &rhs, "MOD")?;
        if b == 0.0 {
            return Err(nh_runtime::Error::runtime("MOD by zero"));
        }
        Ok(Value::Num(a % b))
    }

    fn neg(&mut self, operand: Value) -> nh_runtime::Result<Value> {
        match operand {
            Value::Num(n) => Ok(Value::Num(-n)),
            other => Err(nh_runtime::Error::runtime(format!(
                "cannot negate `{other}`"
            ))),
        }
    }

    fn not(&mut self, operand: Value) -> nh_runtime::Result<Value> {
        let t = <Self as generated::dispatch::Values>::truthy(self, &operand);
        Ok(Self::flag(!t))
    }

    // `AND` and `OR` are bound to `bit_and`/`bit_or`, which are **strict**
    // roles — and that is correct for BASIC, which evaluates both sides. Had
    // they been bound to `and_then`/`or_else`, the generated defaults would
    // short-circuit instead. The table records the choice; the role enforces it.
    fn bit_and(&mut self, lhs: Value, rhs: Value) -> nh_runtime::Result<Value> {
        use generated::dispatch::Values as _;
        Ok(Self::flag(self.truthy(&lhs) && self.truthy(&rhs)))
    }

    fn bit_or(&mut self, lhs: Value, rhs: Value) -> nh_runtime::Result<Value> {
        use generated::dispatch::Values as _;
        Ok(Self::flag(self.truthy(&lhs) || self.truthy(&rhs)))
    }

    /// One method covers the whole comparison tier: the grammar binds all six
    /// spellings to `-> compare`, so the driver hands over a discriminant
    /// instead of generating six near-identical methods.
    fn compare(
        &mut self,
        lhs: Value,
        op: generated::dispatch::CompareOp,
        rhs: Value,
    ) -> nh_runtime::Result<Value> {
        use generated::dispatch::CompareOp as C;

        // `=` and `<>` work on strings too; the ordering comparisons do not.
        if matches!(op, C::Eq | C::LtGt) {
            let equal = lhs == rhs;
            return Ok(Self::flag(if op == C::Eq { equal } else { !equal }));
        }

        let (a, b) = self.nums(&lhs, &rhs, op.spelling())?;
        Ok(Self::flag(match op {
            C::Lt => a < b,
            C::LtEq => a <= b,
            C::Gt => a > b,
            C::GtEq => a >= b,
            C::Eq | C::LtGt => unreachable!("handled above"),
        }))
    }
}

crate::nh_handlers!(Interp);
