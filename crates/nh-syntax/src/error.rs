//! Diagnostics.
//!
//! Rendering lives here rather than in the CLI so that every stage — parsing,
//! import resolution, and later `nh-analysis` — reports through one path and
//! looks identical. DESIGN.md §5.5 makes labelled, well-located errors a
//! feature of the toolkit; the toolkit's own errors should meet that bar.

use crate::source::{SourceMap, Span};
use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

/// A secondary location attached to a diagnostic, such as "first defined here".
#[derive(Clone, Debug)]
pub struct Note {
    pub message: String,
    pub span: Option<Span>,
}

#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub span: Option<Span>,
    pub notes: Vec<Note>,
    /// Rendered after the notes, prefixed with `help:`.
    pub help: Option<String>,
    /// The lint that produced this, when one did.
    ///
    /// Structured rather than only mentioned in a note, so an editor can show
    /// it as a diagnostic code and link to its explanation. The human renderer
    /// still prints it as a note; this is the same fact in a form a machine can
    /// read.
    pub code: Option<String>,
}

impl Diagnostic {
    pub fn error(message: impl Into<String>) -> Self {
        Diagnostic {
            severity: Severity::Error,
            message: message.into(),
            span: None,
            notes: Vec::new(),
            help: None,
            code: None,
        }
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Diagnostic {
            severity: Severity::Warning,
            ..Diagnostic::error(message)
        }
    }

    pub fn at(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }

    pub fn note(mut self, message: impl Into<String>, span: Option<Span>) -> Self {
        self.notes.push(Note {
            message: message.into(),
            span,
        });
        self
    }

    pub fn help(mut self, message: impl Into<String>) -> Self {
        self.help = Some(message.into());
        self
    }

    /// Tags this with the lint that produced it.
    pub fn code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    /// Renders in the rustc style: message, location, source line, caret.
    pub fn render(&self, sm: &SourceMap) -> String {
        let mut out = String::new();
        out.push_str(&format!("{}: {}\n", self.severity.label(), self.message));

        if let Some(span) = self.span {
            render_snippet(&mut out, sm, span, "");
        }

        for note in &self.notes {
            out.push_str(&format!("note: {}\n", note.message));
            if let Some(span) = note.span {
                render_snippet(&mut out, sm, span, "");
            }
        }

        if let Some(help) = &self.help {
            out.push_str(&format!("help: {help}\n"));
        }

        out
    }
}

fn render_snippet(out: &mut String, sm: &SourceMap, span: Span, indent: &str) {
    let lc = sm.line_col(span);
    let line_text = sm.line_text(span);
    let gutter_width = lc.line.to_string().len();
    let pad = " ".repeat(gutter_width);

    out.push_str(&format!(
        "{indent}{pad}--> {}\n",
        sm.location(span)
    ));
    out.push_str(&format!("{indent}{pad} |\n"));
    out.push_str(&format!("{indent}{} | {}\n", lc.line, line_text));

    // Caret width is measured in characters and clamped to the line, so a span
    // running to end-of-file does not paint carets past the text.
    let caret_col = (lc.col as usize).saturating_sub(1);
    let visible = line_text.chars().count().saturating_sub(caret_col);
    let width = sm
        .snippet(span)
        .chars()
        .take_while(|&c| c != '\n')
        .count()
        .clamp(1, visible.max(1));

    out.push_str(&format!(
        "{indent}{pad} | {}{}\n",
        " ".repeat(caret_col),
        "^".repeat(width)
    ));
}

/// One or more diagnostics. Import resolution collects several before giving
/// up, so a run reports every duplicate rather than only the first.
#[derive(Debug)]
pub struct Errors(pub Vec<Diagnostic>);

impl Errors {
    pub fn single(d: Diagnostic) -> Self {
        Errors(vec![d])
    }

    pub fn render(&self, sm: &SourceMap) -> String {
        self.0
            .iter()
            .map(|d| d.render(sm))
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for Errors {
    /// Without a `SourceMap` only the messages can be shown; use
    /// [`Errors::render`] for located output.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for d in &self.0 {
            writeln!(f, "{}: {}", d.severity.label(), d.message)?;
        }
        Ok(())
    }
}

impl std::error::Error for Errors {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceMap;

    #[test]
    fn render_points_at_the_right_column() {
        let mut sm = SourceMap::new();
        let f = sm.add("a.nh", "grammar A;\nrule x = y;\n");
        let d = Diagnostic::error("undefined rule `y`").at(Span::new(f, 20, 21));
        let out = d.render(&sm);
        assert!(out.contains("a.nh:2:10"), "{out}");
        assert!(out.contains("2 | rule x = y;"), "{out}");
        assert!(out.contains("         ^"), "{out}");
    }

    #[test]
    fn caret_does_not_run_past_end_of_line() {
        let mut sm = SourceMap::new();
        let f = sm.add("a.nh", "ab\n");
        // Span deliberately longer than the line.
        let d = Diagnostic::error("boom").at(Span::new(f, 0, 99));
        let out = d.render(&sm);
        let caret_line = out.lines().last().unwrap();
        assert_eq!(caret_line.matches('^').count(), 2, "{out}");
    }
}
