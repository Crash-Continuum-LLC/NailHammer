//! The evaluation context: sources, spans, and diagnostics.
//!
//! `Ctx` keeps a **span stack**, so an error raised anywhere inside a handler is
//! tagged with the innermost node's location automatically. That is the whole
//! point (DESIGN.md §7): no handler threads spans by hand, and adding a new
//! error site cannot forget to.

use crate::diagnostic::Diagnostic;
use crate::error::Error;
use crate::source::{SourceMap, Span};

pub struct Ctx {
    sources: SourceMap,
    /// Innermost last. Pushed on entry to each dispatched node.
    spans: Vec<Span>,
    diagnostics: Vec<Diagnostic>,
}

impl Ctx {
    pub fn new(sources: SourceMap) -> Self {
        Ctx {
            sources,
            spans: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    pub fn sources(&self) -> &SourceMap {
        &self.sources
    }

    pub fn sources_mut(&mut self) -> &mut SourceMap {
        &mut self.sources
    }

    /// The innermost span currently being evaluated.
    pub fn span(&self) -> Option<Span> {
        self.spans.last().copied()
    }

    pub fn enter(&mut self, span: Span) {
        self.spans.push(span);
    }

    pub fn leave(&mut self) {
        self.spans.pop();
    }

    /// Runs `f` with `span` as the innermost location.
    ///
    /// Generated dispatch wraps every handler call in this, which is what makes
    /// `cx.err(..)` locate itself without any handler doing bookkeeping. The
    /// span is popped even if `f` returns `Err`.
    pub fn scoped<T>(&mut self, span: Span, f: impl FnOnce(&mut Self) -> T) -> T {
        self.enter(span);
        let out = f(self);
        self.leave();
        out
    }

    /// The source text of the node being evaluated.
    ///
    /// Handlers take plain parameters and no longer hold a view, so this is
    /// where raw text comes from when a binding is not enough.
    pub fn text(&self) -> &str {
        match self.span() {
            Some(s) => self.sources.snippet(s),
            None => "",
        }
    }

    /// An error located at the current span, for use with `map_err` and
    /// friends where a `Result` is the wrong shape.
    pub fn error(&self, message: impl Into<String>) -> Error {
        Error::Runtime {
            message: message.into(),
            span: self.span(),
        }
    }

    /// Raises an error at the current span.
    pub fn err<T>(&mut self, message: impl Into<String>) -> Result<T, Error> {
        Err(Error::Runtime {
            message: message.into(),
            span: self.span(),
        })
    }

    /// Raises a non-local jump: `break`, `continue`, `return`, `goto`.
    ///
    /// The construct that owns the jump catches it where it evaluates a body;
    /// see [`Error::is_signal`]. Reaching the top uncaught is a real error, and
    /// it reports at the span this recorded.
    pub fn signal(&self, label: &'static str) -> Error {
        Error::Signal {
            label,
            span: self.span(),
        }
    }

    /// Records a diagnostic and continues.
    ///
    /// Use this when evaluation can carry on; use [`Ctx::err`] when it cannot.
    pub fn report(&mut self, d: Diagnostic) {
        let span = self.span();
        self.diagnostics.push(d.or_at(span));
    }

    /// Records a diagnostic and returns [`Error::AlreadyReported`], so the
    /// failure propagates without a second message being produced upstream
    /// (DESIGN.md §5.5).
    pub fn report_and_fail<T>(&mut self, d: Diagnostic) -> Result<T, Error> {
        self.report(d);
        Err(Error::AlreadyReported)
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == crate::diagnostic::Severity::Error)
    }

    /// Renders an error the way the diagnostics do, so a returned `Error` and a
    /// reported `Diagnostic` look identical to the user.
    pub fn render(&self, e: &Error) -> String {
        match e {
            Error::AlreadyReported => String::new(),
            Error::Runtime { message, span } => {
                let mut d = Diagnostic::error(message.clone());
                if let Some(s) = span {
                    d = d.at(*s);
                }
                d.render(&self.sources)
            }
            // An uncaught jump is a real error, and naming it is the whole
            // reason the label is a string rather than an opaque tag.
            Error::Signal { label, span } => {
                let mut d = Diagnostic::error(format!(
                    "`{label}` is not inside anything that handles it"
                ));
                if let Some(s) = span {
                    d = d.at(*s);
                }
                d.render(&self.sources)
            }
        }
    }

    pub fn render_all(&self) -> String {
        self.diagnostics
            .iter()
            .map(|d| d.render(&self.sources))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceMap;

    fn ctx() -> (Ctx, Span) {
        let mut sm = SourceMap::new();
        let f = sm.add("t.txt", "let x = a + 1;\n");
        (Ctx::new(sm), Span::new(f, 8, 13))
    }

    #[test]
    fn errors_pick_up_the_innermost_span() {
        let (mut cx, span) = ctx();
        let e: Error = cx
            .scoped(span, |cx| cx.err::<()>("type mismatch"))
            .unwrap_err();
        let out = cx.render(&e);
        assert!(out.contains("t.txt:1:9"), "{out}");
        assert!(out.contains("type mismatch"), "{out}");
    }

    #[test]
    fn spans_pop_even_when_the_body_fails() {
        let (mut cx, span) = ctx();
        let _ = cx.scoped(span, |cx| cx.err::<()>("boom"));
        assert_eq!(cx.span(), None, "the stack must unwind");
    }

    #[test]
    fn an_explicit_span_is_not_overwritten_by_the_stack() {
        let (mut cx, span) = ctx();
        let other = Span::new(span.file, 0, 3);
        cx.scoped(span, |cx| {
            cx.report(Diagnostic::error("explicit").at(other));
        });
        assert_eq!(cx.diagnostics()[0].span, Some(other));
    }
}
