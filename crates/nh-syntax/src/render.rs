//! Renders an [`Ast`] back into `.nh`-shaped text for `nh check`.
//!
//! Printing normalised source rather than a debug dump makes the output useful
//! twice: it shows what NailHammer understood, and — because it is close to
//! round-trippable — a surprising re-print is itself a bug report. Expression
//! parenthesisation is precedence-aware so the printed form reparses to the
//! same tree.

use crate::ast::*;
use std::fmt::Write;

pub fn render(ast: &Ast) -> String {
    let mut out = String::new();

    if let Some(name) = &ast.grammar_name {
        let _ = writeln!(out, "grammar {};\n", name.value);
    }

    for u in &ast.uses {
        let _ = writeln!(out, "use operators::{};", u.preset.value);
    }
    if !ast.uses.is_empty() {
        out.push('\n');
    }

    if let Some(mode) = &ast.keywords_case {
        let word = match mode.value {
            CaseMode::Insensitive => "case-insensitive",
            CaseMode::Sensitive => "case-sensitive",
        };
        let _ = writeln!(out, "keywords {word};\n");
    }

    for block in &ast.precedence {
        render_precedence(&mut out, block);
    }

    for s in &ast.skips {
        let _ = writeln!(out, "skip {} = {};", s.name.value, expr(&s.body, Prec::Top));
    }
    if !ast.skips.is_empty() {
        out.push('\n');
    }

    for t in &ast.tokens {
        let mut flags = String::new();
        if t.atomic {
            flags.push_str("@ ");
        }
        if t.case_insensitive {
            flags.push_str("case-insensitive ");
        }
        let _ = writeln!(
            out,
            "token {} = {}{};",
            t.name.value,
            flags,
            expr(&t.body, Prec::Top)
        );
    }
    if !ast.tokens.is_empty() {
        out.push('\n');
    }

    for r in &ast.reserved {
        let words: Vec<String> = r.words.iter().map(|w| quote(&w.value)).collect();
        let _ = writeln!(
            out,
            "reserved from {} {{ {} }}",
            r.token.value,
            words.join(" ")
        );
    }
    if !ast.reserved.is_empty() {
        out.push('\n');
    }

    for b in &ast.boundaries {
        let _ = writeln!(
            out,
            "boundary {} = {};",
            b.token.value,
            expr(&b.body, Prec::Top)
        );
    }
    if !ast.boundaries.is_empty() {
        out.push('\n');
    }

    for g in &ast.guards {
        let words: Vec<String> = g.words.iter().map(|w| quote(&w.value)).collect();
        let _ = writeln!(out, "guard from {} {{ {} }}", g.token.value, words.join(" "));
    }
    if !ast.guards.is_empty() {
        out.push('\n');
    }

    for rule in &ast.rules {
        let lead = if rule.silent { "silent rule" } else { "rule" };
        let _ = writeln!(out, "{lead} {}", rule.name.value);
        for (i, alt) in rule.alternatives.iter().enumerate() {
            let lead = if i == 0 { "=" } else { "|" };
            let mut line = format!("  {lead} {}", expr(&alt.body, Prec::Top));
            if let Some(label) = &alt.label {
                let _ = write!(line, " -> {}", label.value);
                if alt.place {
                    line.push_str(" place");
                }
            }
            let _ = writeln!(out, "{line}");
        }
        let _ = writeln!(out, "  ;");
    }
    if !ast.rules.is_empty() {
        out.push('\n');
    }

    for r in &ast.recovers {
        let _ = writeln!(
            out,
            "recover {} sync {};",
            r.rule.value,
            expr(&r.sync, Prec::Top)
        );
    }

    for a in &ast.allows {
        let _ = writeln!(out, "allow {} in {};", a.lint.value, a.rule.value);
    }

    for e in &ast.expects {
        let target: Vec<&str> = e.target.iter().map(|t| t.value.as_str()).collect();
        let _ = writeln!(
            out,
            "expect {} in {} as {};",
            quote(&e.literal.value),
            target.join("."),
            quote(&e.message.value)
        );
    }

    out
}

/// Renders one alternative back to `.nh`, for quoting in generated docs.
///
/// Seeing the grammar fragment a view came from is the difference between
/// "what is `view.name()`?" and not having to ask.
pub fn alternative_source(alt: &Alternative) -> String {
    let mut out = expr(&alt.body, Prec::Top);
    if let Some(label) = &alt.label {
        out.push_str(&format!(" -> {}", label.value));
        if alt.place {
            out.push_str(" place");
        }
    }
    out
}

fn render_precedence(out: &mut String, block: &PrecedenceBlock) {
    let kw = if block.is_override {
        "precedence override"
    } else {
        "precedence"
    };
    let _ = writeln!(out, "{kw} {{");

    for entry in &block.entries {
        match entry {
            PrecEntry::Atom { rule, .. } => {
                let _ = writeln!(out, "  atom {};", rule.value);
            }
            PrecEntry::Remove { ops, .. } => {
                let list: Vec<String> = ops.iter().map(op_ref).collect();
                let _ = writeln!(out, "  remove {};", list.join(" "));
            }
            PrecEntry::Op(op) => {
                let fixity = match op.fixity.value {
                    Fixity::Left => "left",
                    Fixity::Right => "right",
                    Fixity::Prefix => "prefix",
                    Fixity::Postfix => "postfix",
                };
                let ops: Vec<String> = op.ops.iter().map(op_ref).collect();
                let mut line = format!("  {fixity} {}", ops.join(" | "));

                if let Some(p) = &op.placement {
                    let dir = match p.direction {
                        Direction::Above => "above",
                        Direction::Below => "below",
                    };
                    let _ = write!(line, " {dir} {}", quote(&p.anchor.value));
                }
                if !op.lazy.is_empty() {
                    let names: Vec<&str> = op.lazy.iter().map(|l| l.value.as_str()).collect();
                    let _ = write!(line, " lazy({})", names.join(", "));
                }
                if let Some(role) = &op.role {
                    let _ = write!(line, " -> {}", role.value);
                }
                let _ = writeln!(out, "{line};");
            }
        }
    }

    let _ = writeln!(out, "}}\n");
}

fn op_ref(op: &OpRef) -> String {
    if op.word {
        format!("word {}", quote(&op.literal.value))
    } else {
        quote(&op.literal.value)
    }
}

/// Binding strength, so the printed form reparses to the same tree.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Prec {
    /// Choice level — anything may appear unparenthesised.
    Top,
    /// Inside a sequence: a bare choice must be parenthesised.
    Seq,
    /// Under a repetition or lookahead: sequences and choices both must be.
    Unary,
}

fn expr(e: &Expr, ctx: Prec) -> String {
    match &e.kind {
        ExprKind::Choice(parts) => {
            let body = parts
                .iter()
                .map(|p| expr(p, Prec::Seq))
                .collect::<Vec<_>>()
                .join(" | ");
            if ctx >= Prec::Seq {
                format!("({body})")
            } else {
                body
            }
        }
        ExprKind::Seq(parts) => {
            let body = parts
                .iter()
                .map(|p| expr(p, Prec::Seq))
                .collect::<Vec<_>>()
                .join(" ");
            if ctx >= Prec::Unary {
                format!("({body})")
            } else {
                body
            }
        }
        ExprKind::Repeat { inner, kind } => {
            let suffix = match kind {
                RepeatKind::ZeroOrMore => "*",
                RepeatKind::OneOrMore => "+",
                RepeatKind::Optional => "?",
            };
            format!("{}{suffix}", expr(inner, Prec::Unary))
        }
        ExprKind::Lookahead { inner, negative } => {
            let prefix = if *negative { "!" } else { "&" };
            format!("{prefix}{}", expr(inner, Prec::Unary))
        }
        ExprKind::Bind { name, inner, lazy } => {
            let prefix = if *lazy { "lazy " } else { "" };
            // A binding under a repetition or lookahead must be parenthesised:
            // `inner:X*` reparses as `Bind(inner, Repeat(X))`, but this node is
            // `Repeat(Bind(inner, X))`. Different trees, same characters.
            let body = format!("{prefix}{}:{}", name.value, expr(inner, Prec::Unary));
            if ctx >= Prec::Unary {
                format!("({body})")
            } else {
                body
            }
        }
        ExprKind::Literal {
            value,
            case_insensitive,
        } => {
            if *case_insensitive {
                format!("^{}", quote(value))
            } else {
                quote(value)
            }
        }
        ExprKind::CharRange { lo, hi } => format!("{}..{}", quote(lo), quote(hi)),
        ExprKind::Ref(name) => name.clone(),
    }
}

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
            '\0' => out.push_str("\\0"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}
