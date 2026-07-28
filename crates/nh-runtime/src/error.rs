//! The error type generated handlers return.

use crate::source::Span;
use std::fmt;

/// Non-exhaustive on purpose: a language's own control flow arrives as a new
/// variant, and a `match` in user code should not break when one is added.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// A diagnostic has already been recorded for this failure.
    ///
    /// Propagates without producing a second message. This is how DESIGN.md
    /// §5.5's cascade suppression works: one bad expression yields one
    /// diagnostic, not that plus every consequence of it. It is also what a
    /// recovery error node lowers to once M5 lands.
    AlreadyReported,

    Runtime {
        message: String,
        span: Option<Span>,
    },

    /// A **non-local jump** the target language defines: `break`, `continue`,
    /// `return`, `goto`.
    ///
    /// `?` propagation is already exactly the unwinding such a jump needs; the
    /// only thing missing was a variant that is not an error. The runtime never
    /// interprets `label` — it propagates the signal and, if it reaches the top
    /// uncaught, reports it against that name.
    ///
    /// Any **value** the jump carries belongs on the interpreter, which is the
    /// only thing that knows what its values are. `RETURN x` stores `x` and
    /// then signals; the frame that catches it takes the value back out.
    ///
    /// ```ignore
    /// // raising
    /// return Err(cx.signal("break"));
    ///
    /// // catching, in whatever handler owns the construct
    /// match body.eval(host, cx) {
    ///     Err(e) if e.is_signal("break") => break,
    ///     other => { other?; }
    /// }
    /// ```
    Signal {
        label: &'static str,
        span: Option<Span>,
    },
}

impl Error {
    pub fn runtime(message: impl Into<String>) -> Self {
        Error::Runtime {
            message: message.into(),
            span: None,
        }
    }

    pub fn at(mut self, span: Span) -> Self {
        match &mut self {
            Error::Runtime { span: s, .. } | Error::Signal { span: s, .. } => *s = Some(span),
            Error::AlreadyReported => {}
        }
        self
    }

    /// A non-local jump named `label`.
    ///
    /// Prefer [`crate::Ctx::signal`], which fills in where it was raised.
    pub fn signal(label: &'static str) -> Self {
        Error::Signal { label, span: None }
    }

    /// Whether this is the signal a construct is waiting for.
    ///
    /// Comparing by name rather than by type keeps the runtime out of the
    /// business of knowing what jumps a language has.
    pub fn is_signal(&self, label: &str) -> bool {
        matches!(self, Error::Signal { label: l, .. } if *l == label)
    }

    /// An operator or construct the target language does not implement.
    ///
    /// The default body of every generated `Operators` method returns this, so
    /// a language implements only what it supports and the rest reports
    /// honestly instead of silently misbehaving.
    pub fn unsupported(what: &str) -> Self {
        Error::runtime(format!("`{what}` is not supported in this language"))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::AlreadyReported => write!(f, "(already reported)"),
            Error::Runtime { message, .. } => write!(f, "{message}"),
            Error::Signal { label, .. } => {
                write!(f, "`{label}` here has nothing to jump to")
            }
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::FileId;

    /// A signal is not an error the runtime understands — it is one the
    /// *language* defines, so matching is by name.
    #[test]
    fn a_signal_is_recognised_by_its_label() {
        let e = Error::signal("break");
        assert!(e.is_signal("break"));
        assert!(!e.is_signal("continue"), "labels must not collide");
        assert!(!Error::runtime("boom").is_signal("break"));
        assert!(!Error::AlreadyReported.is_signal("break"));
    }

    /// `.at` locates a signal the same way it locates a runtime error, so an
    /// uncaught jump still reports somewhere useful.
    #[test]
    fn a_signal_carries_a_location() {
        let span = Span::new(FileId(0), 3, 8);
        let e = Error::signal("return").at(span);
        assert!(matches!(e, Error::Signal { span: Some(s), .. } if s == span));
    }

    /// The message names the jump, which is why the label is a string.
    #[test]
    fn an_uncaught_signal_says_which_one_it_was() {
        assert_eq!(
            Error::signal("break").to_string(),
            "`break` here has nothing to jump to"
        );
    }
}
