//! The individual passes.

use std::collections::HashSet;

use nh_syntax::ast::{Alternative, Ast, Expr, ExprKind, RepeatKind};
use nh_syntax::{Diagnostic, Severity, Spanned};

use crate::first::{
    folds_case, leading_literal, leading_refs, literal_text, nullable, Rules,
};
use crate::{lint, Allowed};

/// Every lint name, for `nh check --explain` and for validating `allow`.
pub const LINTS: &[(&str, &str)] = &[
    ("left-recursion", "a rule that can reach itself without consuming input"),
    ("nullable-repetition", "a repetition whose body can match nothing"),
    ("shadow", "an earlier alternative that makes a later one unreachable"),
    ("unreachable-alternative", "an alternative after one that always matches"),
    ("duplicate-binding", "the same binding name twice in one sequence"),
    ("unused", "a rule or token nothing refers to"),
    ("recover-sync", "a `recover` sync point that can match nothing"),
    ("silent-binding", "a binding onto a rule that produces no node"),
];

// ---------------------------------------------------------------------------

/// Left recursion is not a hazard in a PEG — it is a non-starter. Pest rejects
/// it, and a rule that can reach itself without consuming input would loop.
pub fn left_recursion(ast: &Ast, allowed: &Allowed, out: &mut Vec<Diagnostic>) {
    let rules = Rules::new(ast);

    for rule in &ast.rules {
        let name = &rule.name.value;
        if allowed.is_allowed("left-recursion", name) {
            continue;
        }

        let mut seen = HashSet::new();
        if let Some(path) = reaches_itself(name, name, &rules, &mut seen) {
            let via = if path.len() > 1 {
                format!(" via {}", path.join(" -> "))
            } else {
                String::new()
            };
            out.push(
                lint(
                    Severity::Error,
                    "left-recursion",
                    name,
                    format!("rule `{name}` is left-recursive{via}"),
                )
                .at(rule.name.span)
                .help(
                    "PEGs cannot express left recursion. If this is an operator \
                     grammar, delete the recursive rule and let the operator table \
                     supply `expr` — that is what it is for.",
                ),
            );
        }
    }
}

/// Depth-first search over leading references.
fn reaches_itself(
    start: &str,
    current: &str,
    rules: &Rules<'_>,
    seen: &mut HashSet<String>,
) -> Option<Vec<String>> {
    let rule = rules.by_name.get(current)?;

    let mut refs = Vec::new();
    for alt in &rule.alternatives {
        leading_refs(&alt.body, rules, &mut refs);
    }

    for r in refs {
        if r == start {
            return Some(vec![current.to_string()]);
        }
        if !seen.insert(r.clone()) {
            continue;
        }
        if let Some(mut path) = reaches_itself(start, &r, rules, seen) {
            path.insert(0, current.to_string());
            return Some(path);
        }
    }
    None
}

// ---------------------------------------------------------------------------

/// `(a?)*` never terminates: the inner match succeeds without consuming, so the
/// repetition runs forever. Pest will hang rather than fail.
pub fn nullable_repetition(ast: &Ast, allowed: &Allowed, out: &mut Vec<Diagnostic>) {
    let rules = Rules::new(ast);

    let check = |owner: &str, e: &Expr, out: &mut Vec<Diagnostic>| {
        if allowed.is_allowed("nullable-repetition", owner) {
            return;
        }
        walk(e, &mut |node| {
            let ExprKind::Repeat { inner, kind } = &node.kind else {
                return;
            };
            if !matches!(kind, RepeatKind::ZeroOrMore | RepeatKind::OneOrMore) {
                return;
            }
            let mut visiting = HashSet::new();
            if nullable(inner, &rules, &mut visiting) {
                out.push(
                    lint(
                        Severity::Error,
                        "nullable-repetition",
                        owner,
                        "this repetition can match nothing, so it never terminates",
                    )
                    .at(node.span)
                    .help(
                        "the repeated expression can succeed without consuming input; \
                         remove the inner `?`/`*`, or the outer repetition",
                    ),
                );
            }
        });
    };

    for r in &ast.rules {
        for alt in &r.alternatives {
            check(&r.name.value, &alt.body, out);
        }
    }
    for t in &ast.tokens {
        check(&t.name.value, &t.body, out);
    }
}

// ---------------------------------------------------------------------------

/// The headline lint: an earlier alternative that makes a later one
/// unreachable.
///
/// Reported only when it is **certain** — the earlier alternative matches a
/// fixed string that is a strict prefix of what the later one must start with.
/// That is the classic `"a" | "ab"` hazard, where the first alternative
/// succeeds on the prefix and the second never runs.
///
/// Note that `"a" X | "ab" Y` is *not* shadowing: if `X` fails the whole
/// alternative fails and the PEG backtracks into the second. Shadowing needs
/// the earlier alternative to actually *succeed* on less input, which is why
/// the check requires it to be entirely literal.
pub fn shadowed_alternatives(ast: &Ast, allowed: &Allowed, out: &mut Vec<Diagnostic>) {
    // `keywords case-insensitive` folds every literal in the grammar, so
    // comparisons must fold too — otherwise `"LET"` would not be seen to shadow
    // `"letter"` in a BASIC-style grammar, which is exactly where it matters.
    let folds_globally = matches!(
        ast.keywords_case.as_ref().map(|m| m.value),
        Some(nh_syntax::ast::CaseMode::Insensitive)
    );

    for rule in &ast.rules {
        let name = &rule.name.value;
        if allowed.is_allowed("shadow", name) {
            continue;
        }

        for (i, earlier) in rule.alternatives.iter().enumerate() {
            let Some(prefix) = literal_text(&earlier.body) else {
                continue;
            };
            if prefix.is_empty() {
                continue;
            }
            let fold_a = folds_globally || folds_case(&earlier.body);

            for later in rule.alternatives.iter().skip(i + 1) {
                let Some(lead) = leading_literal(&later.body) else {
                    continue;
                };
                let fold = fold_a || folds_case(&later.body);

                if !starts_with(&lead, &prefix, fold) || eq(&lead, &prefix, fold) {
                    continue;
                }

                out.push(
                    lint(
                        Severity::Warning,
                        "shadow",
                        name,
                        format!(
                            "this alternative is unreachable: an earlier one matches \
                             `{prefix}`, which is a prefix of `{lead}`"
                        ),
                    )
                    .at(later.span)
                    .note("the earlier alternative is here", Some(earlier.span))
                    .help(
                        "ordered choice takes the first match, so put the longer \
                         alternative first",
                    ),
                );
            }
        }
    }
}

fn starts_with(haystack: &str, needle: &str, fold: bool) -> bool {
    if fold {
        haystack
            .to_ascii_lowercase()
            .starts_with(&needle.to_ascii_lowercase())
    } else {
        haystack.starts_with(needle)
    }
}

fn eq(a: &str, b: &str, fold: bool) -> bool {
    if fold {
        a.eq_ignore_ascii_case(b)
    } else {
        a == b
    }
}

// ---------------------------------------------------------------------------

/// An alternative that can match the empty string always succeeds, so nothing
/// after it can ever run.
pub fn unreachable_alternatives(ast: &Ast, allowed: &Allowed, out: &mut Vec<Diagnostic>) {
    let rules = Rules::new(ast);

    for rule in &ast.rules {
        let name = &rule.name.value;
        if allowed.is_allowed("unreachable-alternative", name) {
            continue;
        }

        for (i, alt) in rule.alternatives.iter().enumerate() {
            if i + 1 == rule.alternatives.len() {
                break;
            }
            let mut visiting = HashSet::new();
            if !nullable(&alt.body, &rules, &mut visiting) {
                continue;
            }
            out.push(
                lint(
                    Severity::Error,
                    "unreachable-alternative",
                    name,
                    format!(
                        "this alternative can match nothing, so the {} after it \
                         can never run",
                        rule.alternatives.len() - i - 1
                    ),
                )
                .at(alt.span)
                .help("move it last, or make it consume something"),
            );
            break;
        }
    }
}

// ---------------------------------------------------------------------------

/// The same binding name twice in one sequence produces two tagged nodes, but
/// the generated accessor returns only the first.
pub fn duplicate_bindings(ast: &Ast, allowed: &Allowed, out: &mut Vec<Diagnostic>) {
    for rule in &ast.rules {
        let name = &rule.name.value;
        if allowed.is_allowed("duplicate-binding", name) {
            continue;
        }
        for alt in &rule.alternatives {
            let mut dups = Vec::new();
            scope_bindings(&alt.body, &mut dups);

            for (dup, first) in dups {
                out.push(
                    lint(
                        Severity::Warning,
                        "duplicate-binding",
                        name,
                        format!(
                            "`{}` is bound twice in the same sequence; the accessor \
                             will return only the first",
                            dup.value
                        ),
                    )
                    .at(dup.span)
                    .note("first bound here", Some(first))
                    .help("give them different names, or bind the repetition instead"),
                );
            }
        }
    }
}

/// Returns the bindings visible in this scope, recording any that collide
/// *within* it.
///
/// Scoping is the whole point. Two branches of a choice may each bind `name` —
/// only one of them matches, so that is correct and must not be reported.
/// A sequence binding `name` twice really does produce two tagged nodes while
/// the generated accessor returns one, so that must be.
fn scope_bindings(
    e: &Expr,
    dups: &mut Vec<(Spanned<String>, nh_syntax::Span)>,
) -> Vec<Spanned<String>> {
    match &e.kind {
        ExprKind::Seq(parts) => {
            let mut names: Vec<Spanned<String>> = Vec::new();
            for p in parts {
                for n in scope_bindings(p, dups) {
                    match names.iter().find(|m| m.value == n.value) {
                        Some(first) => dups.push((n, first.span)),
                        None => names.push(n),
                    }
                }
            }
            names
        }
        ExprKind::Choice(parts) => {
            // Separate scopes. Collect the union for the parent, but never
            // compare across branches.
            let mut union: Vec<Spanned<String>> = Vec::new();
            for p in parts {
                for n in scope_bindings(p, dups) {
                    if !union.iter().any(|m| m.value == n.value) {
                        union.push(n);
                    }
                }
            }
            union
        }
        ExprKind::Bind { name, inner, .. } => {
            let mut names = vec![name.clone()];
            names.extend(scope_bindings(inner, dups));
            names
        }
        // A repetition is its own scope: repeated bindings are expected to
        // occur many times, and the accessor returns all of them.
        ExprKind::Repeat { .. } => Vec::new(),
        // A lookahead consumes nothing, so nothing inside it reaches the tree.
        ExprKind::Lookahead { .. } => Vec::new(),
        _ => Vec::new(),
    }
}

// ---------------------------------------------------------------------------

/// Rules and tokens nothing refers to.
///
/// The first-declared rule is exempt: it is the conventional entry point, and
/// nothing is expected to reference it.
pub fn unused_definitions(
    ast: &Ast,
    operator_atom: Option<&str>,
    allowed: &Allowed,
    out: &mut Vec<Diagnostic>,
) {
    let mut referenced: HashSet<String> = HashSet::new();

    // The operator driver folds over this rule, so it is referenced even though
    // nothing in the grammar text names it.
    if let Some(atom) = operator_atom {
        referenced.insert(atom.to_string());
    }

    fn note(e: &Expr, referenced: &mut HashSet<String>) {
        walk(e, &mut |node| {
            if let ExprKind::Ref(name) = &node.kind {
                referenced.insert(name.clone());
            }
        });
    }
    for r in &ast.rules {
        for alt in &r.alternatives {
            note(&alt.body, &mut referenced);
        }
    }
    for t in &ast.tokens {
        note(&t.body, &mut referenced);
    }
    for s in &ast.skips {
        note(&s.body, &mut referenced);
    }
    for r in &ast.recovers {
        note(&r.sync, &mut referenced);
        referenced.insert(r.rule.value.clone());
    }
    for r in &ast.reserved {
        referenced.insert(r.token.value.clone());
    }
    // The operator table's `atom` rule is referenced by generated code.
    for block in &ast.precedence {
        for entry in &block.entries {
            if let nh_syntax::ast::PrecEntry::Atom { rule, .. } = entry {
                referenced.insert(rule.value.clone());
            }
        }
    }

    let entry = ast.rules.first().map(|r| r.name.value.clone());

    for r in &ast.rules {
        let name = &r.name.value;
        if Some(name.clone()) == entry
            || referenced.contains(name)
            || allowed.is_allowed("unused", name)
        {
            continue;
        }
        out.push(
            lint(
                Severity::Warning,
                "unused",
                name,
                format!("rule `{name}` is never referenced"),
            )
            .at(r.name.span),
        );
    }

    for t in &ast.tokens {
        let name = &t.name.value;
        if referenced.contains(name) || allowed.is_allowed("unused", name) {
            continue;
        }
        out.push(
            lint(
                Severity::Warning,
                "unused",
                name,
                format!("token `{name}` is never referenced"),
            )
            .at(t.name.span),
        );
    }
}

// ---------------------------------------------------------------------------

/// A binding whose target is a `silent` rule.
///
/// A silent rule produces no node, so there is nothing for the tag to attach
/// to. Pest rejects this outright — but its error points at generated `.pest`
/// and says "tags on silent rules will not appear in the output", which tells
/// you nothing about which grammar line caused it.
pub fn silent_binding(ast: &Ast, allowed: &Allowed, out: &mut Vec<Diagnostic>) {
    let silent: HashSet<&str> = ast
        .rules
        .iter()
        .filter(|r| r.silent)
        .map(|r| r.name.value.as_str())
        .collect();
    if silent.is_empty() {
        return;
    }

    for rule in &ast.rules {
        let name = &rule.name.value;
        if allowed.is_allowed("silent-binding", name) {
            continue;
        }
        for alt in &rule.alternatives {
            walk(&alt.body, &mut |node| {
                let ExprKind::Bind { name: binding, inner, .. } = &node.kind else {
                    return;
                };
                let target = match &inner.kind {
                    ExprKind::Ref(t) => t,
                    // `x:silent_rule*` binds the repetition, same problem.
                    ExprKind::Repeat { inner, .. } => match &inner.kind {
                        ExprKind::Ref(t) => t,
                        _ => return,
                    },
                    _ => return,
                };
                if !silent.contains(target.as_str()) {
                    return;
                }
                out.push(
                    lint(
                        Severity::Error,
                        "silent-binding",
                        name,
                        format!(
                            "`{}` binds `{target}`, which is a `silent` rule and \
                             produces no node to bind to",
                            binding.value
                        ),
                    )
                    .at(binding.span)
                    .help(format!(
                        "drop `silent` from `{target}`, or bind what it matches instead"
                    )),
                );
            });
        }
    }
}

/// A `recover` whose sync expression can match the empty string.
///
/// The generated error node is `(!(sync) ~ ANY)+ ~ (sync)?`. If `sync` matches
/// empty then `!(sync)` never succeeds, so the error node can never match and
/// recovery silently does nothing — the grammar compiles, parses, and simply
/// fails to recover, with nothing pointing at why.
pub fn recover_sync(ast: &Ast, allowed: &Allowed, out: &mut Vec<Diagnostic>) {
    let rules = Rules::new(ast);

    for rec in &ast.recovers {
        let name = &rec.rule.value;
        if allowed.is_allowed("recover-sync", name) {
            continue;
        }
        let mut visiting = HashSet::new();
        if !nullable(&rec.sync, &rules, &mut visiting) {
            continue;
        }
        out.push(
            lint(
                Severity::Error,
                "recover-sync",
                name,
                format!("the sync point for `{name}` can match nothing, so recovery never fires"),
            )
            .at(rec.span)
            .help(
                "a sync point must consume something; use a concrete terminator                  such as `\";\"` or `\"}\"`",
            ),
        );
    }
}

/// An `allow` naming a lint that does not exist silences nothing, which is
/// worse than not writing it — the author believes they are covered.
pub fn unknown_allows(ast: &Ast, out: &mut Vec<Diagnostic>) {
    for a in &ast.allows {
        if LINTS.iter().any(|(name, _)| *name == a.lint.value) {
            continue;
        }
        out.push(
            Diagnostic::error(format!("unknown lint `{}`", a.lint.value))
                .at(a.lint.span)
                .help(format!(
                    "available lints: {}",
                    LINTS
                        .iter()
                        .map(|(n, _)| *n)
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
        );
    }
}

// ---------------------------------------------------------------------------

fn walk(e: &Expr, f: &mut impl FnMut(&Expr)) {
    f(e);
    match &e.kind {
        ExprKind::Seq(parts) | ExprKind::Choice(parts) => {
            for p in parts {
                walk(p, f);
            }
        }
        ExprKind::Repeat { inner, .. }
        | ExprKind::Lookahead { inner, .. }
        | ExprKind::Bind { inner, .. } => walk(inner, f),
        _ => {}
    }
}

/// Kept for readability of the passes above.
#[allow(dead_code)]
fn alternative_span(a: &Alternative) -> nh_syntax::Span {
    a.span
}
