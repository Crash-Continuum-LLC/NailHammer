//! Diagnostics for target programs.

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
    pub help: Option<String>,
}

impl Diagnostic {
    pub fn error(message: impl Into<String>) -> Self {
        Diagnostic {
            severity: Severity::Error,
            message: message.into(),
            span: None,
            notes: Vec::new(),
            help: None,
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

    /// Sets the span only if one is not already attached.
    ///
    /// `Ctx` uses this so an explicitly-located error is never overwritten by
    /// the ambient span stack.
    pub fn or_at(mut self, span: Option<Span>) -> Self {
        if self.span.is_none() {
            self.span = span;
        }
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

    pub fn render(&self, sm: &SourceMap) -> String {
        let mut out = String::new();
        out.push_str(&format!("{}: {}\n", self.severity.label(), self.message));

        if let Some(span) = self.span {
            snippet(&mut out, sm, span);
        }
        for note in &self.notes {
            out.push_str(&format!("note: {}\n", note.message));
            if let Some(span) = note.span {
                snippet(&mut out, sm, span);
            }
        }
        if let Some(help) = &self.help {
            out.push_str(&format!("help: {help}\n"));
        }
        out
    }
}

fn snippet(out: &mut String, sm: &SourceMap, span: Span) {
    let lc = sm.line_col(span);
    let line = sm.line_text(span);
    let pad = " ".repeat(lc.line.to_string().len());

    out.push_str(&format!("{pad}--> {}\n", sm.location(span)));
    out.push_str(&format!("{pad} |\n"));
    out.push_str(&format!("{} | {line}\n", lc.line));

    let col = (lc.col as usize).saturating_sub(1);
    let visible = line.chars().count().saturating_sub(col);
    let width = sm
        .snippet(span)
        .chars()
        .take_while(|&c| c != '\n')
        .count()
        .clamp(1, visible.max(1));

    out.push_str(&format!(
        "{pad} | {}{}\n",
        " ".repeat(col),
        "^".repeat(width)
    ));
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.severity.label(), self.message)
    }
}
