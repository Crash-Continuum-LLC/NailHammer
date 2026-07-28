//! Nullability and leading-literal analysis.
//!
//! Both are deliberately **under**-approximations in the direction that keeps
//! lints quiet: `nullable` says yes only when a match on the empty string is
//! certain, and `leading_literal` returns a string only when the start of an
//! expression is a fixed sequence of characters.
//!
//! Being wrong in the other direction would mean a false warning about
//! ordered-choice shadowing, and a determinism lint that cries wolf is worse
//! than none.

use std::collections::{HashMap, HashSet};

use nh_syntax::ast::{Ast, Expr, ExprKind, RepeatKind, RuleDef};

/// What a rule name refers to, for nullability.
pub struct Rules<'a> {
    pub by_name: HashMap<&'a str, &'a RuleDef>,
    pub tokens: HashSet<&'a str>,
}

impl<'a> Rules<'a> {
    pub fn new(ast: &'a Ast) -> Self {
        Rules {
            by_name: ast.rules.iter().map(|r| (r.name.value.as_str(), r)).collect(),
            tokens: ast.tokens.iter().map(|t| t.name.value.as_str()).collect(),
        }
    }
}

/// Whether an expression can match the empty string.
///
/// Unknown references count as **not** nullable, which is the quiet direction:
/// a false "nullable" would produce a spurious infinite-loop error.
pub fn nullable(e: &Expr, rules: &Rules<'_>, visiting: &mut HashSet<String>) -> bool {
    match &e.kind {
        ExprKind::Seq(parts) => parts.iter().all(|p| nullable(p, rules, visiting)),
        ExprKind::Choice(parts) => parts.iter().any(|p| nullable(p, rules, visiting)),
        ExprKind::Repeat { inner, kind } => match kind {
            RepeatKind::ZeroOrMore | RepeatKind::Optional => true,
            RepeatKind::OneOrMore => nullable(inner, rules, visiting),
        },
        // A lookahead consumes nothing, so it always "matches empty".
        ExprKind::Lookahead { .. } => true,
        ExprKind::Bind { inner, .. } => nullable(inner, rules, visiting),
        ExprKind::Literal { value, .. } => value.is_empty(),
        ExprKind::CharRange { .. } => false,
        ExprKind::Ref(name) => {
            // SOI/EOI match without consuming.
            if matches!(name.as_str(), "SOI" | "EOI") {
                return true;
            }
            if rules.tokens.contains(name.as_str()) {
                return false;
            }
            if !visiting.insert(name.clone()) {
                // Recursive: treat as non-nullable to avoid claiming a loop
                // that the left-recursion pass reports properly.
                return false;
            }
            let out = match rules.by_name.get(name.as_str()) {
                Some(r) => r
                    .alternatives
                    .iter()
                    .any(|a| nullable(&a.body, rules, visiting)),
                None => false,
            };
            visiting.remove(name);
            out
        }
    }
}

/// The fixed characters an expression must begin with, if any.
///
/// Returns `None` the moment the start is not a literal — a rule reference, a
/// character range, a choice — because any guess there would risk a false
/// shadowing warning.
pub fn leading_literal(e: &Expr) -> Option<String> {
    match &e.kind {
        ExprKind::Seq(parts) => {
            let mut out = String::new();
            for p in parts {
                match leading_literal(p) {
                    Some(s) => {
                        out.push_str(&s);
                        // Only keep extending while each part is *entirely* a
                        // literal; otherwise this is as far as certainty goes.
                        if !is_pure_literal(p) {
                            break;
                        }
                    }
                    None => break,
                }
            }
            (!out.is_empty()).then_some(out)
        }
        ExprKind::Bind { inner, .. } => leading_literal(inner),
        ExprKind::Literal { value, .. } => (!value.is_empty()).then(|| value.clone()),
        // `x?` and `x*` may match nothing, so nothing is guaranteed to lead.
        ExprKind::Repeat {
            inner,
            kind: RepeatKind::OneOrMore,
        } => leading_literal(inner),
        _ => None,
    }
}

/// Whether an expression is nothing but literal text — so its whole match is
/// known exactly.
pub fn is_pure_literal(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Seq(parts) => parts.iter().all(is_pure_literal),
        ExprKind::Bind { inner, .. } => is_pure_literal(inner),
        ExprKind::Literal { .. } => true,
        _ => false,
    }
}

/// The complete text an expression matches, when it is entirely literal.
pub fn literal_text(e: &Expr) -> Option<String> {
    is_pure_literal(e).then(|| leading_literal(e).unwrap_or_default())
}

/// Whether the expression folds case, so comparisons must too.
pub fn folds_case(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Seq(parts) | ExprKind::Choice(parts) => parts.iter().any(folds_case),
        ExprKind::Bind { inner, .. }
        | ExprKind::Repeat { inner, .. }
        | ExprKind::Lookahead { inner, .. } => folds_case(inner),
        ExprKind::Literal {
            case_insensitive, ..
        } => *case_insensitive,
        _ => false,
    }
}

/// References reachable at the *start* of an expression.
///
/// A reference counts as leading only if everything before it can match empty —
/// which is exactly the condition for left recursion.
pub fn leading_refs(e: &Expr, rules: &Rules<'_>, out: &mut Vec<String>) {
    match &e.kind {
        ExprKind::Seq(parts) => {
            for p in parts {
                leading_refs(p, rules, out);
                let mut visiting = HashSet::new();
                if !nullable(p, rules, &mut visiting) {
                    break;
                }
            }
        }
        ExprKind::Choice(parts) => {
            for p in parts {
                leading_refs(p, rules, out);
            }
        }
        ExprKind::Repeat { inner, .. } | ExprKind::Bind { inner, .. } => {
            leading_refs(inner, rules, out)
        }
        // A lookahead does not consume, so it cannot drive left recursion.
        ExprKind::Lookahead { .. } => {}
        ExprKind::Ref(name) => out.push(name.clone()),
        _ => {}
    }
}
