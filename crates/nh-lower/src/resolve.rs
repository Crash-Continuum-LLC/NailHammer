//! Reference resolution and the identifier-continuation derivation.
//!
//! This is not the full analysis suite from DESIGN.md §5.1 — shadowing,
//! nullable repetition, and the determinism lints are M4. What lives here is
//! only what lowering *needs* to emit a valid `.pest`: knowing whether a name
//! is a rule, a token, or a builtin, and knowing where an identifier ends.

use std::collections::{HashMap, HashSet};

use nh_syntax::ast::{Ast, Expr, ExprKind};
use nh_syntax::{Diagnostic, Span};

/// Structural builtins: the vocabulary a `.nh` grammar is written *in*.
///
/// Shadowing these would take away the only way to express "any character" or
/// "start of input", so redefining one is a hard error.
pub const PEST_RESERVED: &[&str] = &[
    "ANY", "SOI", "EOI", "NEWLINE", "WHITESPACE", "COMMENT", "PUSH", "POP", "POP_ALL", "PEEK",
    "PEEK_ALL", "DROP",
];

/// Builtins that are also natural token names.
///
/// `NUMBER` and `LETTER` are Unicode character properties in pest, but they are
/// what anyone would call their number and letter tokens. Rather than forbid
/// the obvious name, NailHammer renames the emitted rule and rewrites every
/// reference — see [`Resolution::pest_name`].
///
/// This is not cosmetic. Pest rejects a **tag on any reference whose name is a
/// builtin**, even when the user has defined a rule with that name — so
/// `value:NUMBER` fails to compile with a message about built-in rules that
/// points nowhere near the grammar file. Renaming removes the collision
/// entirely.
pub const PEST_SHADOWABLE: &[&str] = &[
    "ASCII", "ASCII_DIGIT", "ASCII_NONZERO_DIGIT", "ASCII_BIN_DIGIT", "ASCII_OCT_DIGIT",
    "ASCII_HEX_DIGIT", "ASCII_ALPHA", "ASCII_ALPHA_LOWER", "ASCII_ALPHA_UPPER",
    "ASCII_ALPHANUMERIC", "LETTER", "NUMBER", "UPPERCASE_LETTER", "LOWERCASE_LETTER",
    "ALPHABETIC", "DIGIT_PROPERTY", "WHITE_SPACE",
];

fn is_builtin(name: &str) -> bool {
    PEST_RESERVED.contains(&name) || PEST_SHADOWABLE.contains(&name)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DefKind {
    Token,
    Rule,
    Skip,
}

/// A `reserved from` or `guard from` target, for shared diagnostics.
struct TokenRef<'a> {
    token: &'a nh_syntax::Spanned<String>,
    what: &'static str,
}

pub struct Resolution {
    pub defined: HashMap<String, DefKind>,
    /// Token → the expression describing what may *continue* an identifier of
    /// that token, used to build keyword boundary guards.
    pub continuations: HashMap<String, Vec<String>>,
    /// User-facing name → the name actually emitted into the `.pest`.
    ///
    /// Identity for everything except definitions that collide with a
    /// shadowable pest builtin, which are suffixed.
    pub renamed: HashMap<String, String>,
    /// Tokens declared `case-insensitive`. Bindings onto these get an accessor
    /// that exposes `.key()` as well as `.text()` (DESIGN.md §5.3).
    pub case_insensitive_tokens: HashSet<String>,
}

impl Resolution {
    pub fn kind(&self, name: &str) -> Option<DefKind> {
        self.defined.get(name).copied()
    }

    /// The name to emit for a reference or definition.
    pub fn pest_name(&self, name: &str) -> String {
        match self.renamed.get(name) {
            Some(n) => n.clone(),
            None => match self.defined.get(name) {
                Some(DefKind::Skip) => crate::names::skip(name),
                _ => name.to_string(),
            },
        }
    }
}

/// Checks every reference in the grammar and derives continuation classes.
///
/// `operator_atom` is the rule named by the table's `atom` entry; when a table
/// is present, `expr` is a legal reference even though no rule defines it.
pub fn resolve(
    ast: &Ast,
    has_operator_table: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> Resolution {
    let mut defined: HashMap<String, DefKind> = HashMap::new();

    let mut renamed: HashMap<String, String> = HashMap::new();

    let define = |name: &str,
                      span: Span,
                      kind: DefKind,
                      defined: &mut HashMap<String, DefKind>,
                      renamed: &mut HashMap<String, String>,
                      diagnostics: &mut Vec<Diagnostic>| {
        // Skips are emitted as `nh_skip_<name>` and unioned into pest's
        // WHITESPACE, so `skip WHITESPACE = ...` collides with nothing — and it
        // is the name people naturally reach for.
        if kind != DefKind::Skip {
            if PEST_RESERVED.contains(&name) {
                diagnostics.push(
                    Diagnostic::error(format!(
                        "`{name}` is a pest built-in and cannot be redefined"
                    ))
                    .at(span)
                    .help("this name is part of the vocabulary grammars are written in"),
                );
                return;
            }
            if PEST_SHADOWABLE.contains(&name) {
                // Emit under a suffixed name and rewrite every reference. Pest
                // rejects a tag on any reference whose *name* is a builtin,
                // even a user-defined one, so `value:NUMBER` would otherwise
                // fail with a message pointing nowhere near the grammar.
                let mut candidate = format!("{name}_");
                while defined.contains_key(&candidate) {
                    candidate.push('_');
                }
                renamed.insert(name.to_string(), candidate);
            }
        }
        defined.insert(name.to_string(), kind);
    };

    for s in &ast.skips {
        define(&s.name.value, s.name.span, DefKind::Skip, &mut defined, &mut renamed, diagnostics);
    }
    for t in &ast.tokens {
        define(&t.name.value, t.name.span, DefKind::Token, &mut defined, &mut renamed, diagnostics);
    }
    for r in &ast.rules {
        define(&r.name.value, r.name.span, DefKind::Rule, &mut defined, &mut renamed, diagnostics);
    }

    // `expr` is supplied by the operator system, not by a rule (DESIGN.md §3).
    let mut known: HashSet<String> = defined.keys().cloned().collect();
    if has_operator_table {
        known.insert("expr".to_string());
    }

    let check = |e: &Expr, diagnostics: &mut Vec<Diagnostic>| {
        walk(e, &mut |node| {
            if let ExprKind::Ref(name) = &node.kind {
                if known.contains(name) || is_builtin(name) {
                    return;
                }
                let d = Diagnostic::error(format!("undefined reference `{name}`")).at(node.span);
                let d = if name == "expr" {
                    d.help(
                        "`expr` comes from the operator system; add `use operators::<preset>;` \
                         or a `precedence { .. }` block with an `atom` entry",
                    )
                } else {
                    d.help("define it as a `rule` or `token`, or check the spelling")
                };
                diagnostics.push(d);
            }
        });
    };

    for s in &ast.skips {
        check(&s.body, diagnostics);
    }
    for t in &ast.tokens {
        check(&t.body, diagnostics);
    }
    for r in &ast.rules {
        for alt in &r.alternatives {
            check(&alt.body, diagnostics);
        }
    }
    for r in &ast.recovers {
        check(&r.sync, diagnostics);
        if !known.contains(&r.rule.value) {
            diagnostics.push(
                Diagnostic::error(format!("`recover` names unknown rule `{}`", r.rule.value))
                    .at(r.rule.span),
            );
        }
    }

    // Continuation classes for every token with reserved *or* guarded words:
    // both need to know where an identifier ends.
    let mut continuations = HashMap::new();
    let guarded_tokens = ast
        .reserved
        .iter()
        .map(|r| (&r.token, "reserved from"))
        .chain(ast.guards.iter().map(|g| (&g.token, "guard from")));

    for (token_ref, what) in guarded_tokens {
        let token = &token_ref.value;
        let reserved = TokenRef {
            token: token_ref,
            what,
        };

        // An explicit `boundary` wins over the derivation.
        if let Some(b) = ast.boundaries.iter().find(|b| &b.token.value == token) {
            let mut parts = Vec::new();
            collect_terminals(&b.body, &mut parts);
            continuations.insert(token.clone(), parts);
            continue;
        }

        match ast.tokens.iter().find(|t| &t.name.value == token) {
            Some(def) => {
                let (cont, derived) = continuation_class(&def.body);

                // The derivation reads the operands of the token's repetitions.
                // With no repetition it falls back to *every* terminal, which
                // over-approximates: a character legal only in first position
                // would be treated as able to continue the token, and the guard
                // would reject a boundary it should accept.
                //
                // Say so rather than being quietly imprecise — that was the
                // whole complaint about this heuristic.
                if !derived && !cont.is_empty() {
                    diagnostics.push(
                        Diagnostic::warning(format!(
                            "cannot derive an identifier boundary for `{token}` \
                             precisely; approximating from every character it can match"
                        ))
                        .at(reserved.token.span)
                        .note(
                            "the boundary is normally read from the token's repeated \
                             part, and this token has none",
                            Some(def.name.span),
                        )
                        .help(format!(
                            "state it outright: `boundary {token} = <what may follow>;`"
                        )),
                    );
                }

                if cont.is_empty() {
                    diagnostics.push(
                        Diagnostic::error(format!(
                            "cannot derive an identifier boundary for token `{token}`"
                        ))
                        .at(reserved.token.span)
                        .note(
                            "a reserved-word guard needs to know which characters may continue \
                             an identifier",
                            Some(def.name.span),
                        )
                        .help("give the token a repeated tail, e.g. `ALPHA (ALNUM | \"_\")*`"),
                    );
                } else {
                    continuations.insert(token.clone(), cont);
                }
            }
            None => diagnostics.push(
                Diagnostic::error(format!(
                    "`{}` names unknown token `{token}`",
                    reserved.what
                ))
                .at(reserved.token.span),
            ),
        }
    }

    let mut res = Resolution {
        defined,
        continuations: HashMap::new(),
        renamed,
        case_insensitive_tokens: ast
            .tokens
            .iter()
            .filter(|t| t.case_insensitive)
            .map(|t| t.name.value.clone())
            .collect(),
    };

    // Continuation fragments are emitted verbatim, so references inside them
    // must use the renamed form too.
    res.continuations = continuations
        .into_iter()
        .map(|(token, parts)| {
            let mapped = parts
                .into_iter()
                .map(|p| {
                    if res.defined.contains_key(&p) {
                        res.pest_name(&p)
                    } else {
                        p
                    }
                })
                .collect();
            (token, mapped)
        })
        .collect();

    res
}

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
        ExprKind::Literal { .. } | ExprKind::CharRange { .. } | ExprKind::Ref(_) => {}
    }
}

/// Derives what may continue an identifier, for keyword boundary guards.
///
/// The rule: collect the operands of every repetition in the token body. For
/// `IDENT = (ALPHA | "_") (ALNUM | "_")*` that yields `ALNUM | "_"` — exactly
/// the tail class, which is what the guard needs.
///
/// Taking the *repeated* part rather than every terminal matters. A union over
/// the whole body would also include characters legal only in first position,
/// making the guard reject boundaries it should accept.
/// Returns the continuation class and whether it was derived *precisely*.
///
/// Precise means it came from the token's repeated part, which is exactly the
/// tail class. `false` means the fallback ran and the result over-approximates.
fn continuation_class(body: &Expr) -> (Vec<String>, bool) {
    let mut out = Vec::new();
    collect_repeated(body, &mut out);
    let derived = !out.is_empty();

    if out.is_empty() {
        // No repetition: the token is a fixed shape, so anything the token can
        // match could in principle continue it. Fall back to every terminal.
        collect_terminals(body, &mut out);
    }
    out.sort();
    out.dedup();
    (out, derived)
}

fn collect_repeated(e: &Expr, out: &mut Vec<String>) {
    match &e.kind {
        ExprKind::Repeat { inner, .. } => collect_terminals(inner, out),
        ExprKind::Seq(parts) | ExprKind::Choice(parts) => {
            for p in parts {
                collect_repeated(p, out);
            }
        }
        ExprKind::Lookahead { inner, .. } | ExprKind::Bind { inner, .. } => {
            collect_repeated(inner, out)
        }
        _ => {}
    }
}

/// Collects terminals as **pest source fragments**, not debug strings — these
/// are emitted verbatim into the generated guard, so they must match the
/// generated token byte for byte.
fn collect_terminals(e: &Expr, out: &mut Vec<String>) {
    match &e.kind {
        ExprKind::Ref(name) => out.push(name.clone()),
        ExprKind::Literal {
            value,
            case_insensitive,
        } => out.push(crate::pest_syntax::string(value, *case_insensitive)),
        ExprKind::CharRange { lo, hi } => {
            if let Some(r) = crate::pest_syntax::char_range(lo, hi) {
                out.push(r);
            }
        }
        ExprKind::Seq(parts) | ExprKind::Choice(parts) => {
            for p in parts {
                collect_terminals(p, out);
            }
        }
        ExprKind::Repeat { inner, .. }
        | ExprKind::Lookahead { inner, .. }
        | ExprKind::Bind { inner, .. } => collect_terminals(inner, out),
    }
}
