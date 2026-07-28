//! The interpreter's value type.

use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Str(String),
    Num(f64),
    Bool(bool),
    Null,
    List(Vec<Value>),
    Table(Vec<(String, Value)>),

    /// A key/value pair, produced by the `entry` handler and consumed by
    /// `document` and `value_table`.
    ///
    /// Intermediates like this are why the trait stack carries a single
    /// associated `Out` rather than a fixed type: every handler in a pass
    /// speaks one language, and that language is the target's business.
    Field(String, Box<Value>),
}

impl Value {
    /// Unwraps a `Field`, which is a programming error to call on anything else.
    pub fn into_field(self) -> Option<(String, Value)> {
        match self {
            Value::Field(k, v) => Some((k, *v)),
            _ => None,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Str(s) => write!(f, "{s:?}"),
            Value::Num(n) => write!(f, "{n}"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Null => write!(f, "null"),
            Value::List(items) => {
                write!(f, "[")?;
                for (i, v) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{v}")?;
                }
                write!(f, "]")
            }
            Value::Table(fields) => {
                write!(f, "{{")?;
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}: {v}")?;
                }
                write!(f, "}}")
            }
            Value::Field(k, v) => write!(f, "{k}: {v}"),
        }
    }
}
