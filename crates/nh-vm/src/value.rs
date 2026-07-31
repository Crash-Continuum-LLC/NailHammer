//! What the machine holds.
//!
//! # The three kinds of sharing (VM-DESIGN.md §7.4)
//!
//! A running program holds three kinds of value and only one of them needs
//! synchronising. That split is the whole performance argument, so it is worth
//! being able to see it in the type:
//!
//! * **machine-local** — a register or temporary. One machine, one thread, and
//!   it never escapes, so it is a plain `Value` with no atomic and no lock.
//! * **immutable shared** — a string, and later code and constants. Held behind
//!   [`Arc`], which costs a refcount and *no lock at all*, because there is no
//!   writer to exclude.
//! * **mutable shared** — a global. Not represented here: it lives in a
//!   [`SharedStore`](crate::store::SharedStore), which is the only place
//!   synchronisation happens.
//!
//! `Value` is `Send + Sync` so it can cross into the shared store, but nothing
//! about being `Send` costs anything until a value actually crosses.

use std::sync::Arc;

#[derive(Clone, Debug, Default)]
pub enum Value {
    #[default]
    Nil,
    Bool(bool),
    Num(f64),
    /// `Arc<str>` rather than `String`: a string handed to two programs is one
    /// allocation and a refcount bump, and it is immutable, so no lock is
    /// needed to read it from either.
    Str(Arc<str>),
}

impl Value {
    pub fn str(s: &str) -> Self {
        Value::Str(Arc::from(s))
    }

    /// The VM's opinion about truth, and the reason a language cannot bring its
    /// own (VM-DESIGN.md §3.5). `JumpIfFalse` asks this and nothing else.
    pub fn truthy(&self) -> bool {
        match self {
            Value::Nil => false,
            Value::Bool(b) => *b,
            Value::Num(n) => *n != 0.0,
            Value::Str(s) => !s.is_empty(),
        }
    }

    pub fn as_num(&self) -> Result<f64, String> {
        match self {
            Value::Num(n) => Ok(*n),
            other => Err(format!("expected a number, got {other:?}")),
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Nil, Value::Nil) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Num(a), Value::Num(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            _ => false,
        }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Nil => write!(f, "nil"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Num(n) => write!(f, "{n}"),
            Value::Str(s) => write!(f, "{s}"),
        }
    }
}

/// Compile-time proof that a value may be shared.
///
/// If `Value` ever stops being `Send + Sync` — an `Rc` slipped into a variant,
/// say — this stops compiling here rather than at some call site in a host that
/// tried to share one.
const _: () = {
    const fn assert_shareable<T: Send + Sync>() {}
    assert_shareable::<Value>();
};
