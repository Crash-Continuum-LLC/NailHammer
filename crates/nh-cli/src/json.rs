//! Machine-readable diagnostics, for editors.
//!
//! `nh check --json` prints one JSON array of diagnostics on stdout and nothing
//! else, so a tool can parse it without stripping human output first. The
//! rendered form on stderr is unchanged and still the default.
//!
//! Written by hand rather than with `serde`, because the shape is small, fixed,
//! and fully covered by tests — and because a JSON dependency in a grammar
//! toolkit earns its place only if it does more than this.

use nh_syntax::{Diagnostic, Severity, SourceMap, Span};

/// A whole diagnostic run as one JSON array.
pub fn diagnostics(sm: &SourceMap, list: &[Diagnostic]) -> String {
    let items: Vec<String> = list.iter().map(|d| diagnostic(sm, d)).collect();
    format!("[{}]", items.join(","))
}

fn diagnostic(sm: &SourceMap, d: &Diagnostic) -> String {
    let mut fields = vec![
        field("severity", &quote(match d.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        })),
        field("message", &quote(&d.message)),
    ];

    if let Some(code) = &d.code {
        fields.push(field("code", &quote(code)));
    }
    if let Some(span) = d.span {
        fields.push(field("location", &location(sm, span)));
    }
    if let Some(help) = &d.help {
        fields.push(field("help", &quote(help)));
    }
    if !d.notes.is_empty() {
        let notes: Vec<String> = d
            .notes
            .iter()
            .map(|n| {
                let mut f = vec![field("message", &quote(&n.message))];
                if let Some(s) = n.span {
                    f.push(field("location", &location(sm, s)));
                }
                format!("{{{}}}", f.join(","))
            })
            .collect();
        fields.push(field("notes", &format!("[{}]", notes.join(","))));
    }

    format!("{{{}}}", fields.join(","))
}

/// A span as a range an editor can select.
///
/// Lines and columns are **1-based**, matching the rendered output and every
/// compiler a user has seen. An editor that wants 0-based subtracts one, which
/// is a conversion it is already doing for every other tool.
fn location(sm: &SourceMap, span: Span) -> String {
    let start = sm.line_col(span);
    let end = sm.line_col(Span::new(span.file, span.hi, span.hi));

    format!(
        "{{{}}}",
        [
            field("file", &quote(&sm.path(span.file).display().to_string())),
            field("line", &start.line.to_string()),
            field("column", &start.col.to_string()),
            field("endLine", &end.line.to_string()),
            field("endColumn", &end.col.to_string()),
        ]
        .join(",")
    )
}

fn field(name: &str, value: &str) -> String {
    format!("{}:{}", quote(name), value)
}

/// Escapes a string as a JSON literal.
///
/// Diagnostics contain quotes and backslashes constantly — every message that
/// names a `"literal"` or a `\n` escape — so this is the part that has to be
/// right.
fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // Control characters have no literal form in JSON.
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_handles_what_diagnostics_actually_contain() {
        assert_eq!(quote(r#"expected ")""#), r#""expected \")\"""#);
        assert_eq!(quote(r"a \n escape"), r#""a \\n escape""#);
        assert_eq!(quote("two\nlines"), r#""two\nlines""#);
        assert_eq!(quote("tab\there"), r#""tab\there""#);
    }

    #[test]
    fn control_characters_are_escaped_numerically() {
        assert_eq!(quote("\u{1}"), "\"\\u0001\"");
    }

    /// Non-ASCII passes through: JSON is UTF-8 and every consumer reads it.
    #[test]
    fn unicode_is_left_alone() {
        assert_eq!(quote("naïve — ß"), "\"naïve — ß\"");
    }

    #[test]
    fn a_diagnostic_carries_its_lint_code_and_range() {
        let mut sm = SourceMap::new();
        let f = sm.add("t.nh", "rule a = b;\nrule c = d;\n");
        let d = Diagnostic::warning("nothing refers to `a`")
            .at(Span::new(f, 12, 16))
            .code("unused")
            .help("delete it");

        let json = diagnostics(&sm, std::slice::from_ref(&d));
        for expect in [
            r#""severity":"warning""#,
            r#""code":"unused""#,
            r#""line":2"#,
            r#""column":1"#,
            r#""endColumn":5"#,
            r#""help":"delete it""#,
        ] {
            assert!(json.contains(expect), "missing {expect} in:\n{json}");
        }
    }

    #[test]
    fn no_diagnostics_is_an_empty_array() {
        let sm = SourceMap::new();
        assert_eq!(diagnostics(&sm, &[]), "[]");
    }
}
