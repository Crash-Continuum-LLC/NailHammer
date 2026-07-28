//! Generated diagnostics: expectation labels and syntax-error collection.
//!
//! DESIGN.md §5.5. Two halves:
//!
//! * **Better messages.** Pest reports the *rules* it expected, which reads as
//!   rule-name soup. `expect "(" in call as "opening parenthesis of call
//!   arguments"` promotes the literal to its own rule (in lowering) and maps
//!   that rule to a sentence here.
//! * **Recovery.** A `recover` rule parses successfully even when its body
//!   fails, leaving an error node behind. Walking for those nodes yields one
//!   diagnostic per failure — multiple errors per run, with no runtime
//!   backtracking machinery.

use nh_lower::Lowered;
use std::fmt::Write as _;

use crate::{Options, HEADER};

pub fn generate(lowered: &Lowered, opts: &Options) -> String {
    let mut out = String::new();
    out.push_str(HEADER);

    let parser_mod = opts
        .parser_type
        .rsplit_once("::")
        .map(|(m, _)| m)
        .unwrap_or("crate");

    let _ = writeln!(
        out,
        "\n#![allow(dead_code, unused_imports)]\n\n\
         use nh_runtime::{{Diagnostic, FileId, Span}};\n\
         use nh_runtime::pest::Pair;\n\n\
         use {parser_mod}::Rule;\n"
    );

    emit_describe(&mut out, lowered);
    emit_collect(&mut out, lowered);
    emit_parse_error(&mut out, lowered);

    out
}

/// `describe` — human names for the rules a user can see in an error.
fn emit_describe(out: &mut String, lowered: &Lowered) {
    if lowered.expectations.is_empty() {
        let _ = writeln!(
            out,
            "/// This grammar declares no `expect` labels.\n\
             pub fn describe(rule: Rule) -> Option<&'static str> {{\n\
            \x20   let _ = rule;\n\
            \x20   None\n\
             }}\n"
        );
        return;
    }

    let _ = writeln!(
        out,
        "/// A human description of a rule, for error messages.\n\
         ///\n\
         /// Populated from `expect .. as ..` declarations in the grammar. Pest\n\
         /// names the rules it expected; this is what turns those into a sentence.\n\
         pub fn describe(rule: Rule) -> Option<&'static str> {{\n\
        \x20   match rule {{"
    );
    for (rule, message) in &lowered.expectations {
        let _ = writeln!(out, "        Rule::{rule} => Some({message:?}),");
    }
    let _ = writeln!(out, "        _ => None,\n    }}\n}}\n");
}

/// `syntax_errors` — one diagnostic per recovery node.
fn emit_collect(out: &mut String, lowered: &Lowered) {
    let _ = writeln!(
        out,
        "/// Collects every syntax error the parse recovered from.\n\
         ///\n\
         /// A `recover` rule succeeds even when its body fails, leaving behind a\n\
         /// node that stands for the skipped text. Call this after parsing and\n\
         /// before evaluating: it reports **all** the syntax errors at once,\n\
         /// rather than the first one and nothing else.\n\
         pub fn syntax_errors(pair: &Pair<'_, Rule>, file: FileId) -> Vec<Diagnostic> {{\n\
        \x20   let mut out = Vec::new();\n\
        \x20   walk(pair, file, &mut out);\n\
        \x20   out\n\
         }}\n\n\
         fn walk(pair: &Pair<'_, Rule>, file: FileId, out: &mut Vec<Diagnostic>) {{\n\
        \x20   if let Some(d) = error_at(pair, file) {{\n\
        \x20       out.push(d);\n\
        \x20       // Do not descend: the whole subtree is the one failure.\n\
        \x20       return;\n\
        \x20   }}\n\
        \x20   for inner in pair.clone().into_inner() {{\n\
        \x20       walk(&inner, file, out);\n\
        \x20   }}\n\
         }}\n"
    );

    let match_start = out.len();
    let _ = writeln!(
        out,
        "/// Whether this node *is* a recovery node, and the message if so.\n\
         pub fn error_at(pair: &Pair<'_, Rule>, file: FileId) -> Option<Diagnostic> {{\n\
        \x20   let s = pair.as_span();\n\
        \x20   let span = Span::new(file, s.start() as u32, s.end() as u32);\n\
        \x20   match pair.as_rule() {{"
    );

    if lowered.recoveries.is_empty() {
        // Truncate the match we just started and emit a body with no unused
        // bindings: generated code must not warn in the user's project.
        out.truncate(match_start);
        let _ = writeln!(
            out,
            "/// This grammar declares no `recover` rules, so nothing recovers.\n\
             pub fn error_at(pair: &Pair<'_, Rule>, file: FileId) -> Option<Diagnostic> {{\n\
            \x20   let _ = (pair, file);\n\
            \x20   None\n\
             }}\n"
        );
        return;
    }

    for rec in &lowered.recoveries {
        let _ = writeln!(
            out,
            "        Rule::{} => Some(\n\
            \x20           Diagnostic::error(\"could not parse this `{}`\")\n\
            \x20               .at(span)\n\
            \x20               .help(\"skipped to the next sync point and carried on, so errors after this one are real\"),\n\
            \x20       ),",
            rec.error_rule, rec.rule
        );
    }
    if lowered.recoveries.is_empty() {
        let _ = writeln!(
            out,
            "        // This grammar declares no `recover` rules."
        );
    }

    let _ = writeln!(out, "        _ => None,\n    }}\n}}\n");
}

/// `render_parse_error` — pest's error, through the description table.
fn emit_parse_error(out: &mut String, lowered: &Lowered) {
    let _ = lowered;
    let _ = writeln!(
        out,
        "/// Turns pest's parse error into a diagnostic with a readable message.\n\
         ///\n\
         /// Pest lists the rules it expected. Any of them with an `expect`\n\
         /// description contributes a sentence; the rest are named as-is, which is\n\
         /// still better than nothing but is why `expect` exists.\n\
         pub fn render_parse_error(\n\
        \x20   error: &::pest::error::Error<Rule>,\n\
        \x20   file: FileId,\n\
         ) -> Diagnostic {{\n\
        \x20   use ::pest::error::{{ErrorVariant, InputLocation}};\n\n\
        \x20   let (lo, hi) = match error.location {{\n\
        \x20       InputLocation::Pos(p) => (p, p + 1),\n\
        \x20       InputLocation::Span((s, e)) => (s, e),\n\
        \x20   }};\n\
        \x20   let span = Span::new(file, lo as u32, hi as u32);\n\n\
        \x20   let message = match &error.variant {{\n\
        \x20       ErrorVariant::ParsingError {{ positives, .. }} if !positives.is_empty() => {{\n\
        \x20           let mut names: Vec<String> = positives\n\
        \x20               .iter()\n\
        \x20               .map(|r| match describe(*r) {{\n\
        \x20                   Some(text) => text.to_string(),\n\
        \x20                   None => format!(\"`{{r:?}}`\"),\n\
        \x20               }})\n\
        \x20               .collect();\n\
        \x20           names.sort();\n\
        \x20           names.dedup();\n\
        \x20           match names.len() {{\n\
        \x20               1 => format!(\"expected {{}}\", names[0]),\n\
        \x20               _ => {{\n\
        \x20                   let last = names.pop().expect(\"non-empty\");\n\
        \x20                   format!(\"expected {{}}, or {{last}}\", names.join(\", \"))\n\
        \x20               }}\n\
        \x20           }}\n\
        \x20       }}\n\
        \x20       ErrorVariant::ParsingError {{ .. }} => \"unexpected input\".to_string(),\n\
        \x20       ErrorVariant::CustomError {{ message }} => message.clone(),\n\
        \x20   }};\n\n\
        \x20   Diagnostic::error(message).at(span)\n\
         }}\n"
    );
}
