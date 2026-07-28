//! Parsing `.nh` source into [`Ast`].
//!
//! Everything here reads the pair tree by **node tag or rule kind**, never by
//! child position. That is not incidental tidiness — it is the same discipline
//! NailHammer will generate for its users (DESIGN.md §2), applied to itself.
//!
//! In particular, note [`tagged`]: it scans *direct children* rather than
//! calling `Pairs::find_first_tagged`. Pest's tag lookup is built on
//! `.flatten()` and searches the whole subtree, so on a nested rule an outer
//! node would happily find an inner node's identically-named tag. Getting this
//! wrong is silent.

use pest::error::{ErrorVariant, InputLocation};
use pest::iterators::Pair;
use pest::Parser;
use pest_derive::Parser;

use crate::ast::*;
use crate::error::{Diagnostic, Errors};
use crate::source::{FileId, SourceMap, Span, Spanned};

#[derive(Parser)]
#[grammar = "nh.pest"]
struct NhParser;

/// Parses one file's text into an [`Ast`]. Imports are *recorded* but not
/// followed; see `import::resolve`.
pub fn parse_file(sm: &SourceMap, file: FileId) -> Result<Ast, Errors> {
    let text = sm.text(file);
    let mut pairs = NhParser::parse(Rule::file, text)
        .map_err(|e| Errors::single(pest_error_to_diagnostic(e, file)))?;

    let file_pair = pairs.next().expect("file rule always yields one pair");
    Ok(lower_file(file_pair, file))
}

// ---------------------------------------------------------------------------
// Pair-tree navigation
// ---------------------------------------------------------------------------

/// Finds a direct child carrying `tag`.
///
/// Deliberately not `find_first_tagged`, which flattens the whole subtree.
fn tagged<'i>(pair: &Pair<'i, Rule>, tag: &str) -> Option<Pair<'i, Rule>> {
    pair.clone()
        .into_inner()
        .find(|p| p.as_node_tag() == Some(tag))
}

/// Like [`tagged`], but for a tag the grammar guarantees is present.
fn required<'i>(pair: &Pair<'i, Rule>, tag: &str) -> Pair<'i, Rule> {
    tagged(pair, tag).unwrap_or_else(|| {
        unreachable!(
            "nh.pest guarantees tag `{tag}` on rule {:?}; grammar and lowerer are out of sync",
            pair.as_rule()
        )
    })
}

fn child<'i>(pair: &Pair<'i, Rule>, rule: Rule) -> Option<Pair<'i, Rule>> {
    pair.clone().into_inner().find(|p| p.as_rule() == rule)
}

fn children<'i>(pair: &Pair<'i, Rule>, rule: Rule) -> Vec<Pair<'i, Rule>> {
    pair.clone()
        .into_inner()
        .filter(|p| p.as_rule() == rule)
        .collect()
}

fn has(pair: &Pair<'_, Rule>, rule: Rule) -> bool {
    child(pair, rule).is_some()
}

fn span_of(pair: &Pair<'_, Rule>, file: FileId) -> Span {
    let s = pair.as_span();
    Span::new(file, s.start() as u32, s.end() as u32)
}

fn ident_of(pair: &Pair<'_, Rule>, file: FileId) -> Spanned<String> {
    Spanned::new(pair.as_str().to_string(), span_of(pair, file))
}

/// Reads a `string` / `string_ci` pair, unescaping its contents.
fn string_of(pair: &Pair<'_, Rule>, file: FileId) -> Spanned<String> {
    let raw = child(pair, Rule::str_inner)
        .map(|p| p.as_str().to_string())
        .unwrap_or_default();
    Spanned::new(unescape(&raw), span_of(pair, file))
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('0') => out.push('\0'),
            Some('u') => {
                // `u{...}` — the grammar has already validated the shape.
                let mut hex = String::new();
                for c in chars.by_ref() {
                    match c {
                        '{' => continue,
                        '}' => break,
                        _ => hex.push(c),
                    }
                }
                if let Some(ch) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    out.push(ch);
                }
            }
            Some(other) => out.push(other), // `"`, `\`, `/`
            None => out.push('\\'),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Items
// ---------------------------------------------------------------------------

fn lower_file(pair: Pair<'_, Rule>, file: FileId) -> Ast {
    let mut ast = Ast::default();

    for item in pair.into_inner() {
        let span = span_of(&item, file);
        match item.as_rule() {
            Rule::grammar_decl => {
                ast.grammar_name = Some(ident_of(&required(&item, "name"), file));
            }
            Rule::import_item => ast.imports.push(Import {
                path: string_of(&required(&item, "path"), file),
                span,
            }),
            Rule::use_item => ast.uses.push(UsePreset {
                preset: ident_of(&required(&item, "preset"), file),
                span,
            }),
            Rule::keywords_item => {
                let mode = required(&item, "mode");
                let value = if mode.as_str().starts_with("case-insensitive") {
                    CaseMode::Insensitive
                } else {
                    CaseMode::Sensitive
                };
                ast.keywords_case = Some(Spanned::new(value, span_of(&mode, file)));
            }
            Rule::precedence_item => ast.precedence.push(lower_precedence(&item, file, span)),
            Rule::skip_item => ast.skips.push(SkipDef {
                name: ident_of(&required(&item, "name"), file),
                body: lower_alt(&required(&item, "body"), file),
                span,
            }),
            Rule::token_item => ast.tokens.push(TokenDef {
                name: ident_of(&required(&item, "name"), file),
                atomic: has(&item, Rule::atomic_marker),
                case_insensitive: has(&item, Rule::ci_marker),
                body: lower_alt(&required(&item, "body"), file),
                span,
            }),
            Rule::reserved_item => ast.reserved.push(ReservedDef {
                token: ident_of(&required(&item, "token"), file),
                words: children(&item, Rule::string)
                    .iter()
                    .map(|p| string_of(p, file))
                    .collect(),
                span,
            }),
            Rule::guard_item => ast.guards.push(GuardDef {
                token: ident_of(&required(&item, "token"), file),
                words: children(&item, Rule::string)
                    .iter()
                    .map(|p| string_of(p, file))
                    .collect(),
                span,
            }),
            Rule::boundary_item => ast.boundaries.push(BoundaryDef {
                token: ident_of(&required(&item, "token"), file),
                body: lower_alt(&required(&item, "body"), file),
                span,
            }),
            Rule::rule_item => ast.rules.push(RuleDef {
                name: ident_of(&required(&item, "name"), file),
                silent: has(&item, Rule::silent_marker),
                alternatives: lower_alternatives(&required(&item, "body"), file),
                span,
            }),
            Rule::recover_item => ast.recovers.push(RecoverDef {
                rule: ident_of(&required(&item, "rule"), file),
                sync: lower_alt(&required(&item, "sync"), file),
                span,
            }),
            Rule::allow_item => ast.allows.push(AllowDef {
                lint: ident_of(&required(&item, "lint"), file),
                rule: ident_of(&required(&item, "rule"), file),
                span,
            }),
            Rule::expect_item => {
                let target = required(&item, "target");
                ast.expects.push(ExpectDef {
                    literal: string_of(&required(&item, "lit"), file),
                    target: children(&target, Rule::ident)
                        .iter()
                        .map(|p| ident_of(p, file))
                        .collect(),
                    message: string_of(&required(&item, "msg"), file),
                    span,
                });
            }
            Rule::EOI => {}
            other => unreachable!("unexpected item rule {other:?}"),
        }
    }

    ast
}

// ---------------------------------------------------------------------------
// Precedence blocks
// ---------------------------------------------------------------------------

fn lower_precedence(pair: &Pair<'_, Rule>, file: FileId, span: Span) -> PrecedenceBlock {
    let mut entries = Vec::new();

    for entry in pair.clone().into_inner() {
        let espan = span_of(&entry, file);
        match entry.as_rule() {
            // Keyword rules are atomic (see nh.pest), so they surface as pairs.
            Rule::kw_precedence | Rule::override_marker => {}
            Rule::atom_entry => entries.push(PrecEntry::Atom {
                rule: ident_of(&required(&entry, "rule"), file),
                span: espan,
            }),
            Rule::remove_entry => entries.push(PrecEntry::Remove {
                ops: children(&entry, Rule::op_ref)
                    .iter()
                    .map(|p| lower_op_ref(p, file))
                    .collect(),
                span: espan,
            }),
            Rule::op_entry => entries.push(PrecEntry::Op(lower_op_entry(&entry, file, espan))),
            other => unreachable!("unexpected precedence entry {other:?}"),
        }
    }

    PrecedenceBlock {
        is_override: has(pair, Rule::override_marker),
        entries,
        span,
    }
}

fn lower_op_entry(pair: &Pair<'_, Rule>, file: FileId, span: Span) -> OpEntry {
    let fixity_pair = required(pair, "fixity");
    let fixity = match fixity_pair.as_str().trim() {
        "left" => Fixity::Left,
        "right" => Fixity::Right,
        "prefix" => Fixity::Prefix,
        "postfix" => Fixity::Postfix,
        other => unreachable!("nh.pest restricts fixity to four keywords, got {other:?}"),
    };

    let ops = children(&required(pair, "ops"), Rule::op_ref)
        .iter()
        .map(|p| lower_op_ref(p, file))
        .collect();

    let placement = child(pair, Rule::placement).map(|p| {
        let dir_pair = required(&p, "dir");
        Placement {
            direction: if dir_pair.as_str().trim() == "above" {
                Direction::Above
            } else {
                Direction::Below
            },
            anchor: string_of(&required(&p, "anchor"), file),
            span: span_of(&p, file),
        }
    });

    let lazy = child(pair, Rule::lazy_spec)
        .map(|p| {
            children(&p, Rule::ident)
                .iter()
                .map(|i| ident_of(i, file))
                .collect()
        })
        .unwrap_or_default();

    let role = child(pair, Rule::role).map(|p| ident_of(&required(&p, "name"), file));

    OpEntry {
        fixity: Spanned::new(fixity, span_of(&fixity_pair, file)),
        ops,
        placement,
        lazy,
        role,
        span,
    }
}

fn lower_op_ref(pair: &Pair<'_, Rule>, file: FileId) -> OpRef {
    OpRef {
        word: has(pair, Rule::word_marker),
        literal: string_of(&required(pair, "lit"), file),
        span: span_of(pair, file),
    }
}

// ---------------------------------------------------------------------------
// Expressions
// ---------------------------------------------------------------------------

fn lower_alternatives(pair: &Pair<'_, Rule>, file: FileId) -> Vec<Alternative> {
    children(pair, Rule::alternative)
        .iter()
        .map(|alt| {
            let arrow = child(alt, Rule::arrow);
            Alternative {
                body: lower_seq(&required(alt, "body"), file),
                label: arrow
                    .as_ref()
                    .map(|a| ident_of(&required(a, "label"), file)),
                place: arrow
                    .as_ref()
                    .map(|a| has(a, Rule::place_marker))
                    .unwrap_or(false),
                span: span_of(alt, file),
            }
        })
        .collect()
}

fn lower_alt(pair: &Pair<'_, Rule>, file: FileId) -> Expr {
    let span = span_of(pair, file);
    let mut parts: Vec<Expr> = children(pair, Rule::seq)
        .iter()
        .map(|p| lower_seq(p, file))
        .collect();

    if parts.len() == 1 {
        parts.pop().expect("length checked")
    } else {
        Expr {
            kind: ExprKind::Choice(parts),
            span,
        }
    }
}

fn lower_seq(pair: &Pair<'_, Rule>, file: FileId) -> Expr {
    let span = span_of(pair, file);
    let mut parts: Vec<Expr> = children(pair, Rule::labeled)
        .iter()
        .map(|p| lower_labeled(p, file))
        .collect();

    if parts.len() == 1 {
        parts.pop().expect("length checked")
    } else {
        Expr {
            kind: ExprKind::Seq(parts),
            span,
        }
    }
}

fn lower_labeled(pair: &Pair<'_, Rule>, file: FileId) -> Expr {
    let span = span_of(pair, file);
    let inner = lower_rep(
        &child(pair, Rule::rep).expect("nh.pest guarantees a rep in labeled"),
        file,
    );

    match tagged(pair, "name") {
        Some(name) => Expr {
            kind: ExprKind::Bind {
                name: ident_of(&name, file),
                inner: Box::new(inner),
                lazy: has(pair, Rule::lazy_marker),
            },
            span,
        },
        None => inner,
    }
}

fn lower_rep(pair: &Pair<'_, Rule>, file: FileId) -> Expr {
    let span = span_of(pair, file);
    let inner = lower_pre(
        &child(pair, Rule::pre).expect("nh.pest guarantees a pre in rep"),
        file,
    );

    match child(pair, Rule::rep_op) {
        Some(op) => Expr {
            kind: ExprKind::Repeat {
                inner: Box::new(inner),
                kind: match op.as_str() {
                    "*" => RepeatKind::ZeroOrMore,
                    "+" => RepeatKind::OneOrMore,
                    "?" => RepeatKind::Optional,
                    other => unreachable!("nh.pest restricts rep_op, got {other:?}"),
                },
            },
            span,
        },
        None => inner,
    }
}

fn lower_pre(pair: &Pair<'_, Rule>, file: FileId) -> Expr {
    let span = span_of(pair, file);
    let term = pair
        .clone()
        .into_inner()
        .find(|p| p.as_rule() != Rule::pre_op)
        .expect("nh.pest guarantees a term in pre");
    let inner = lower_term(&term, file);

    match child(pair, Rule::pre_op) {
        Some(op) => Expr {
            kind: ExprKind::Lookahead {
                inner: Box::new(inner),
                negative: op.as_str() == "!",
            },
            span,
        },
        None => inner,
    }
}

fn lower_term(pair: &Pair<'_, Rule>, file: FileId) -> Expr {
    let span = span_of(pair, file);
    let kind = match pair.as_rule() {
        Rule::group => {
            return lower_alt(
                &child(pair, Rule::alt).expect("nh.pest guarantees an alt in group"),
                file,
            )
        }
        Rule::char_range => ExprKind::CharRange {
            lo: string_of(&required(pair, "lo"), file).value,
            hi: string_of(&required(pair, "hi"), file).value,
        },
        Rule::string => ExprKind::Literal {
            value: string_of(pair, file).value,
            case_insensitive: false,
        },
        Rule::string_ci => ExprKind::Literal {
            value: string_of(pair, file).value,
            case_insensitive: true,
        },
        Rule::rule_ref => ExprKind::Ref(pair.as_str().trim().to_string()),
        other => unreachable!("unexpected term rule {other:?}"),
    };
    Expr { kind, span }
}

// ---------------------------------------------------------------------------
// Pest error translation
// ---------------------------------------------------------------------------

fn pest_error_to_diagnostic(e: pest::error::Error<Rule>, file: FileId) -> Diagnostic {
    let (lo, hi) = match e.location {
        InputLocation::Pos(p) => (p, p + 1),
        InputLocation::Span((s, en)) => (s, en),
    };

    let message = match &e.variant {
        ErrorVariant::ParsingError { positives, .. } if !positives.is_empty() => {
            let mut names: Vec<&str> = positives.iter().map(|r| describe(*r)).collect();
            names.sort_unstable();
            names.dedup();
            format!("expected {}", join_alternatives(&names))
        }
        ErrorVariant::ParsingError { .. } => "unexpected input".to_string(),
        ErrorVariant::CustomError { message } => message.clone(),
    };

    Diagnostic::error(message).at(Span::new(file, lo as u32, hi as u32))
}

fn join_alternatives(names: &[&str]) -> String {
    match names {
        [] => "something else".to_string(),
        [one] => one.to_string(),
        [a, b] => format!("{a} or {b}"),
        [rest @ .., last] => format!("{}, or {last}", rest.join(", ")),
    }
}

/// Human names for the rules users actually see in errors. This is a small
/// preview of the generated `diagnostics.rs` described in DESIGN.md §5.5.
fn describe(rule: Rule) -> &'static str {
    match rule {
        Rule::file => "a grammar file",
        Rule::grammar_decl => "a `grammar` declaration",
        Rule::import_item => "an `import`",
        Rule::use_item => "a `use operators::` declaration",
        Rule::keywords_item => "a `keywords` declaration",
        Rule::precedence_item => "a `precedence` block",
        Rule::skip_item => "a `skip` definition",
        Rule::token_item => "a `token` definition",
        Rule::reserved_item => "a `reserved from` declaration",
        Rule::guard_item => "a `guard from` declaration",
        Rule::boundary_item => "a `boundary` declaration",
        Rule::rule_item => "a `rule` definition",
        Rule::recover_item => "a `recover` declaration",
        Rule::expect_item => "an `expect` declaration",
        Rule::allow_item => "an `allow` declaration",
        Rule::alternatives | Rule::alternative => "an alternative",
        Rule::alt | Rule::seq | Rule::labeled | Rule::rep | Rule::pre => "an expression",
        Rule::group => "a parenthesised group",
        Rule::string => "a string literal",
        Rule::string_ci => "a case-insensitive string literal",
        Rule::char_range => "a character range",
        Rule::rule_ref | Rule::ident => "an identifier",
        Rule::fixity => "`left`, `right`, `prefix`, or `postfix`",
        Rule::op_ref | Rule::op_list => "an operator literal",
        Rule::atom_entry => "an `atom` entry",
        Rule::remove_entry => "a `remove` entry",
        Rule::op_entry => "an operator entry",
        Rule::role => "a `-> role` binding",
        Rule::lazy_spec => "a `lazy(..)` specification",
        Rule::placement => "an `above`/`below` placement",
        Rule::case_mode => "`case-insensitive` or `case-sensitive`",
        Rule::EOI => "end of file",
        _ => "more input",
    }
}

