//! Lowers a `.nh` grammar to pest source (DESIGN.md milestone M1).
//!
//! What this produces is a `.pest` file that pest can compile and that parses
//! the target language. What it does *not* produce is Rust — views, handler
//! dispatch, and the operator driver are M2 and M3.
//!
//! Three parts of the design show up directly in the output:
//!
//!   * **Bindings become node tags.** `name:IDENT` emits `#name = IDENT`, which
//!     is what lets M2 generate accessors by name instead of by position (§2).
//!   * **Reserved words are guarded in both directions** (§5.3): keyword
//!     literals get an identifier-boundary lookahead, and the identifier token
//!     is taught to reject reserved words.
//!   * **Operator alternations are sorted longest-first** (§5.2), so `<=` is
//!     reachable and `a+++b` munches maximally. Only synthesised alternations
//!     are sorted; the user's own ordered choices are emitted as written.

pub mod names;
pub mod pest_syntax;
pub mod resolve;

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use nh_operators::OperatorTable;
use nh_syntax::ast::{Alternative, Ast, CaseMode, Expr, ExprKind, Fixity, RepeatKind, RuleDef};
use nh_syntax::{Diagnostic, Errors};

use crate::resolve::Resolution;

/// The generated grammar plus the facts M2 will need about it.
pub struct Lowered {
    /// Complete pest source.
    pub pest: String,
    /// Labelled alternative → the pest rule that identifies it. This is the
    /// handler map: at M2 each entry becomes a view type and a handler file.
    pub alternatives: Vec<LoweredAlternative>,
    /// Whether an `expr` rule was emitted. False when the grammar declares no
    /// operators, in which case there is nothing for a driver to fold.
    pub has_expr: bool,
    /// Rules with error recovery: `(rule, error rule name)`.
    ///
    /// Dispatch needs these to report a syntax error and poison the subtree
    /// rather than trying to evaluate a node that stands for "something went
    /// wrong here".
    pub recoveries: Vec<Recovery>,
    /// Literals promoted to their own rule so pest's expected-set can name
    /// them: `(rule name, human description)`.
    pub expectations: Vec<(String, String)>,
    /// Non-fatal diagnostics produced while lowering.
    ///
    /// Lowering can notice things worth saying that do not stop it — an
    /// imprecise identifier boundary, for one. Returning them means a caller
    /// can surface them; before this they were computed and dropped.
    pub diagnostics: Vec<Diagnostic>,
    /// Every rule's node shape, in declaration order (M7: the owned AST).
    pub rules: Vec<LoweredRule>,
    /// Rules that are pure alternations over labelled sub-rules.
    ///
    /// These carry no handler of their own — the alternative that matched does.
    /// Dispatch delegates straight through them, which is why an author writes
    /// `rule value = .. -> string | .. -> number;` and gets a handler per
    /// alternative rather than one big `match` inside a handler for `value`.
    pub wrapper_rules: Vec<String>,
}

/// The shape of one rule, for generating an owned AST type.
///
/// `alternatives` says what handlers exist; this says what *nodes* exist and
/// how they nest, which is a different question. A rule with three labelled
/// alternatives is an enum of three structs; `rule atom = primary;` is an
/// alias; `rule entry = k:IDENT "=" v:value ";" -> entry;` is one struct.
#[derive(Clone, Debug)]
pub struct LoweredRule {
    /// The name as written in the grammar.
    pub name: String,
    /// The pest rule whose pair carries this node.
    pub pest_rule: String,
    pub shape: RuleShape,
    /// Whether the rule recovers, so the AST needs an error variant.
    pub recovers: bool,
}

#[derive(Clone, Debug)]
pub enum RuleShape {
    /// Exactly one labelled alternative: the rule *is* the alternative.
    Single { pest_rule: String },
    /// Several alternatives.
    Choice(Vec<LoweredVariant>),
    /// `rule atom = primary;` — no node of its own, delegates to one child.
    Alias { child: Option<String> },
}

#[derive(Clone, Debug)]
pub enum LoweredVariant {
    /// A labelled alternative with a struct of its own.
    Labelled { label: String, pest_rule: String },
    /// `-> pass`, or unlabelled: whatever its single child evaluates to.
    ///
    /// `child` is the rule it yields when that is determinable — `"(" e:expr ")"`
    /// yields an `expr`. `None` means the alternative binds no single rule, and
    /// the AST cannot name a type for it.
    Transparent { child: Option<String> },
}

pub struct LoweredAlternative {
    pub rule: String,
    pub label: String,
    /// The generated pest rule name, e.g. `stmt_let`.
    pub pest_rule: String,
    /// The alternative as written in the grammar, for generated documentation.
    pub source: String,
    /// Bindings in source order — these become the view's named accessors.
    pub bindings: Vec<Binding>,
    pub place: bool,
}

/// A rule that recovers from a parse failure.
#[derive(Clone, Debug)]
pub struct Recovery {
    /// The user's rule name.
    pub rule: String,
    /// The generated rule that matches the skipped-over text.
    pub error_rule: String,
}

/// One `name:expr` binding, with everything a view accessor needs.
#[derive(Clone, Debug)]
pub struct Binding {
    pub name: String,
    pub cardinality: Cardinality,
    /// The token this binding resolves to, when it resolves to exactly one.
    /// Drives whether the accessor hands back an `Ident` (with `.key()`) or a
    /// plain `Node`.
    pub token: Option<BoundToken>,
    /// `lazy` — the handler receives this unevaluated, as a `Deferred`.
    pub lazy: bool,
    /// The *rule* this binding references, when it references exactly one.
    ///
    /// A binding onto a rule needs `dispatch`; one onto a token needs
    /// `.text()`. Generated documentation says which, because the type alone
    /// (`Node`) does not.
    pub rule_ref: Option<String>,
}

#[derive(Clone, Debug)]
pub struct BoundToken {
    pub name: String,
    pub case_insensitive: bool,
}

/// How many times a binding can match, which decides the accessor's shape:
/// `Node`, `Option<Node>`, or an iterator.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cardinality {
    /// Exactly one. The grammar guarantees it is present.
    One,
    /// Zero or one — under `?`, or in one branch of a choice.
    Optional,
    /// Any number — under `*` or `+`.
    Many,
}

impl Cardinality {
    /// Combines an enclosing context with an inner one.
    ///
    /// `Many` dominates: a binding inside a repetition can occur repeatedly no
    /// matter what else encloses it. `Optional` beats `One` for the same
    /// reason — a binding in one branch of a choice may simply not be there.
    fn combine(self, inner: Cardinality) -> Cardinality {
        match (self, inner) {
            (Cardinality::Many, _) | (_, Cardinality::Many) => Cardinality::Many,
            (Cardinality::Optional, _) | (_, Cardinality::Optional) => Cardinality::Optional,
            _ => Cardinality::One,
        }
    }
}

/// Lowers a resolved grammar and operator table into pest source.
pub fn lower(ast: &Ast, table: &OperatorTable) -> Result<Lowered, Errors> {
    let mut diagnostics = Vec::new();
    let has_table = table.atom_rule.is_some();
    let res = resolve::resolve(ast, has_table, &mut diagnostics);

    let mut ctx = Ctx {
        ast,
        table,
        res: &res,
        fold_keywords: matches!(
            ast.keywords_case.as_ref().map(|m| m.value),
            Some(CaseMode::Insensitive)
        ),
        reserved_words: HashMap::new(),
        guarded_words: HashMap::new(),
        keyword_rules: HashMap::new(),
        emitted_continuations: HashSet::new(),
        emitted_expr: false,
        wrapper_rules: Vec::new(),
        rule_shapes: Vec::new(),
        recoveries: Vec::new(),
        expectations: Vec::new(),
        expect_rules: HashMap::new(),
        current_rule: None,
        current_label: None,
        generated: Vec::new(),
        alloc: names::Allocator::default(),
        collected_alternatives: Vec::new(),
        diagnostics: &mut diagnostics,
    };

    ctx.collect_reserved_words();
    let body = ctx.emit_all();
    let has_expr = ctx.emitted_expr;
    let wrapper_rules = std::mem::take(&mut ctx.wrapper_rules);
    let rules = std::mem::take(&mut ctx.rule_shapes);
    let recoveries = std::mem::take(&mut ctx.recoveries);
    let expectations = std::mem::take(&mut ctx.expectations);
    let generated = std::mem::take(&mut ctx.generated);
    let alternatives = ctx.take_alternatives();

    if diagnostics
        .iter()
        .any(|d| d.severity == nh_syntax::Severity::Error)
    {
        return Err(Errors(diagnostics));
    }

    let mut pest = String::new();
    let name = ast
        .grammar_name
        .as_deref()
        .map(String::as_str)
        .unwrap_or("grammar");
    let _ = writeln!(
        pest,
        "// Generated by NailHammer from the `{name}` grammar. DO NOT EDIT.\n\
         // Edit the .nh source and re-run `nh build`.\n"
    );
    pest.push_str(&body);
    if !generated.is_empty() {
        pest.push_str(
            "\n// ---------------------------------------------------------------------------\n\
             // Generated support rules\n\
             // ---------------------------------------------------------------------------\n\n",
        );
        for line in &generated {
            pest.push_str(line);
            pest.push('\n');
        }
    }

    Ok(Lowered {
        pest,
        diagnostics,
        alternatives,
        rules,
        has_expr,
        recoveries,
        expectations,
        wrapper_rules,
    })
}

struct Ctx<'a> {
    ast: &'a Ast,
    table: &'a OperatorTable,
    res: &'a Resolution,
    fold_keywords: bool,
    /// token → reserved literals (declared plus auto-added word operators).
    ///
    /// These get a boundary guard **and** are rejected by the identifier token.
    reserved_words: HashMap<String, Vec<String>>,
    /// token → guarded literals.
    ///
    /// Boundary-guarded only: still usable as identifiers. This is what a
    /// grammar with *contextual* keywords needs — `.nh` itself has `atom` as
    /// both a precedence keyword and an ordinary rule name.
    guarded_words: HashMap<String, Vec<String>>,
    /// literal → generated guarded-keyword rule name.
    keyword_rules: HashMap<String, String>,
    /// Tokens whose `nh_cont_*` rule has already been emitted.
    emitted_continuations: HashSet<String>,
    /// Set once an `expr` rule is written.
    emitted_expr: bool,
    wrapper_rules: Vec<String>,
    rule_shapes: Vec<LoweredRule>,
    recoveries: Vec<Recovery>,
    expectations: Vec<(String, String)>,
    /// `(target, literal)` -> generated expectation rule.
    ///
    /// Keyed by target as well as literal, because `expect ")" in call` and
    /// `expect ")" in group` are different messages about the same character.
    /// Keying on the literal alone silently dropped the second one.
    expect_rules: HashMap<(String, String), String>,
    /// The rule and alternative label currently being lowered, so a literal can
    /// be matched against an `expect` target.
    current_rule: Option<String>,
    current_label: Option<String>,
    generated: Vec<String>,
    alloc: names::Allocator,
    collected_alternatives: Vec<LoweredAlternative>,
    diagnostics: &'a mut Vec<Diagnostic>,
}

impl Ctx<'_> {
    fn take_alternatives(&mut self) -> Vec<LoweredAlternative> {
        std::mem::take(&mut self.collected_alternatives)
    }

    /// Gathers reserved words per token, adding word operators automatically
    /// (DESIGN.md §6.5: `word "AND"` is reserved without restating it).
    fn collect_reserved_words(&mut self) {
        let ast = self.ast;
        for r in &ast.reserved {
            self.reserved_words
                .entry(r.token.value.clone())
                .or_default()
                .extend(r.words.iter().map(|w| w.value.clone()));
        }
        for g in &ast.guards {
            self.guarded_words
                .entry(g.token.value.clone())
                .or_default()
                .extend(g.words.iter().map(|w| w.value.clone()));
        }

        let word_ops: Vec<String> = self
            .table
            .operators()
            .filter(|(_, o)| o.word)
            .map(|(_, o)| o.literal.clone())
            .collect();

        if word_ops.is_empty() {
            return;
        }

        let identifier_token = ast
            .reserved
            .first()
            .map(|r| r.token.value.clone())
            .or_else(|| ast.guards.first().map(|g| g.token.value.clone()));

        match identifier_token {
            Some(token) => {
                // Word operators are reserved, not merely guarded: `AND` cannot
                // also be a variable name (DESIGN.md §6.5).
                let entry = self.reserved_words.entry(token).or_default();
                for w in word_ops {
                    if !entry.contains(&w) {
                        entry.push(w);
                    }
                }
            }
            None => {
                let span = self
                    .table
                    .operators()
                    .find(|(_, o)| o.word)
                    .and_then(|(_, o)| o.span);
                let d = Diagnostic::error(
                    "word operators need an identifier token to be guarded against",
                )
                .help(
                    "add `reserved from <TOKEN> { }` or `guard from <TOKEN> { }` naming \
                     the grammar's identifier token; word operators are added \
                     automatically",
                );
                self.diagnostics.push(match span {
                    Some(s) => d.at(s),
                    None => d,
                });
            }
        }
    }
}

// ---------------------------------------------------------------------------

impl<'a> Ctx<'a> {
    fn emit_all(&mut self) -> String {
        let mut out = String::new();

        self.collect_expectations();
        self.emit_skips(&mut out);
        self.emit_tokens(&mut out);
        self.emit_expr(&mut out);
        self.emit_rules(&mut out);

        out
    }

    /// Promotes each `expect` literal to its own rule.
    ///
    /// Pest reports the *rules* it expected, never bare literals, so a literal
    /// written inline can never appear in an error message. Giving it a rule
    /// name is what lets `expect "(" in call as "opening parenthesis"` turn
    /// rule-name soup into a sentence.
    fn collect_expectations(&mut self) {
        let ast = self.ast;
        for e in &ast.expects {
            let literal = &e.literal.value;
            let target: Vec<&str> = e.target.iter().map(|t| t.value.as_str()).collect();
            let key = (target.join("."), literal.clone());

            if !self.expect_target_exists(&target) {
                self.diagnostics.push(
                    Diagnostic::error(format!(
                        "`expect` names unknown target `{}`",
                        key.0
                    ))
                    .at(e.span)
                    .help(
                        "the target is a rule name, or `rule.label` for one \
                         alternative of it",
                    ),
                );
                continue;
            }

            if let Some(previous) = self.expect_rules.get(&key) {
                let _ = previous;
                self.diagnostics.push(
                    Diagnostic::error(format!(
                        "`{}` already has an `expect` message in `{}`",
                        literal, key.0
                    ))
                    .at(e.span)
                    .help("two messages for the same literal in the same place; keep one"),
                );
                continue;
            }

            let name = self.alloc.alloc(&names::expectation(&key.0, literal));
            let lit = pest_syntax::string(literal, self.fold_keywords);
            // Silent: the rule exists only so pest can *name* it in an error.
            // A non-silent rule would produce a pair and change the shape of
            // the tree, which quietly broke transparent delegation the first
            // time this was written — `"(" expr ")"` went from one child to two.
            self.generated.push(format!("{name} = _{{ {lit} }}"));
            self.expect_rules.insert(key, name.clone());
            self.expectations.push((name, e.message.value.clone()));
        }
    }

    /// Whether an `expect` target names a real rule, or `rule.label` a real
    /// alternative of one.
    fn expect_target_exists(&self, target: &[&str]) -> bool {
        let Some(rule) = self.ast.rules.iter().find(|r| r.name.value == target[0]) else {
            return false;
        };
        match target.len() {
            1 => true,
            2 => rule
                .alternatives
                .iter()
                .any(|a| a.label.as_deref().map(String::as_str) == Some(target[1])),
            _ => false,
        }
    }

    /// The expectation rule for `literal` in the position being lowered.
    ///
    /// A more specific `rule.label` target wins over a whole-rule one.
    fn expectation_for(&self, literal: &str) -> Option<String> {
        let rule = self.current_rule.as_ref()?;
        if let Some(label) = &self.current_label {
            let key = (format!("{rule}.{label}"), literal.to_string());
            if let Some(name) = self.expect_rules.get(&key) {
                return Some(name.clone());
            }
        }
        self.expect_rules
            .get(&(rule.clone(), literal.to_string()))
            .cloned()
    }

    /// All `skip` definitions are unioned into pest's `WHITESPACE`.
    ///
    /// Pest only honours `WHITESPACE` and `COMMENT`, but `.nh` allows any
    /// number of skips under any names. Unioning them keeps that promise
    /// without depending on a user happening to name one `COMMENT`.
    fn emit_skips(&mut self, out: &mut String) {
        let ast = self.ast;
        if ast.skips.is_empty() {
            return;
        }

        out.push_str("// Implicit skipping\n");
        let mut parts = Vec::new();
        for s in &ast.skips {
            let rule = names::skip(&s.name.value);
            let body = self.expr(&s.body, false);
            let _ = writeln!(out, "{rule} = _{{ {body} }}");
            parts.push(rule);
        }
        let _ = writeln!(out, "WHITESPACE = _{{ {} }}\n", parts.join(" | "));
    }

    fn emit_tokens(&mut self, out: &mut String) {
        let ast = self.ast;
        if ast.tokens.is_empty() {
            return;
        }
        out.push_str("// Tokens\n");

        for t in &ast.tokens {
            let body = self.expr(&t.body, t.case_insensitive);

            // `@` is atomic: no implicit whitespace, and no inner nodes.
            // Anything else is **compound-atomic** (`$`): still no implicit
            // whitespace, but inner rules keep producing nodes.
            //
            // A plain `{ }` would let whitespace be skipped *inside* a token,
            // so `token WRAPPED = "<" INNER ">";` would match `< abc >`. That
            // is never what `token` means, and it was the behaviour until this
            // was fixed. It also closes `.nh`'s last gap against `nh.pest`,
            // which uses `${ }` for string literals.
            let modifier = if t.atomic { "@" } else { "$" };

            // A token with reserved words must reject them, or `let` parses as
            // an identifier and the keyword alternative never fires.
            let words = self.reserved_words.get(&t.name.value).cloned();
            let body = match words {
                Some(words) if !words.is_empty() => {
                    let guard = self.emit_reserved_guard(&t.name.value, &words);
                    format!("!{guard} ~ {}", pest_syntax::group(&body))
                }
                _ => body,
            };

            let _ = writeln!(
                out,
                "{} = {modifier}{{ {body} }}",
                self.res.pest_name(&t.name.value)
            );
        }
        out.push('\n');
    }

    /// Emits `nh_cont_TOKEN` and `nh_reserved_TOKEN`, returning the latter.
    fn emit_reserved_guard(&mut self, token: &str, words: &[String]) -> String {
        let cont = self.continuation_rule(token);
        let reserved = names::reserved(token);

        let mut sorted: Vec<&String> = words.iter().collect();
        // Longest first, for the same reason operator alternations are sorted.
        sorted.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));

        let alts = sorted
            .iter()
            .map(|w| pest_syntax::string(w, self.fold_keywords))
            .collect::<Vec<_>>()
            .join(" | ");

        self.generated
            .push(format!("{reserved} = @{{ ({alts}) ~ !{cont} }}"));
        reserved
    }

    /// Emits `nh_cont_TOKEN` once, returning its name.
    fn continuation_rule(&mut self, token: &str) -> String {
        let name = names::continuation(token);
        if !self.emitted_continuations.insert(token.to_string()) {
            return name;
        }
        let parts = self
            .res
            .continuations
            .get(token)
            .cloned()
            .unwrap_or_default();
        let body = if parts.is_empty() {
            "ASCII_ALPHANUMERIC | \"_\"".to_string()
        } else {
            parts.join(" | ")
        };
        self.generated.push(format!("{name} = _{{ {body} }}"));
        name
    }

    /// Whether any rule body mentions `expr`.
    fn references_expr(&self) -> bool {
        fn walk(e: &Expr) -> bool {
            match &e.kind {
                ExprKind::Ref(name) => name == "expr",
                ExprKind::Seq(parts) | ExprKind::Choice(parts) => parts.iter().any(walk),
                ExprKind::Repeat { inner, .. }
                | ExprKind::Lookahead { inner, .. }
                | ExprKind::Bind { inner, .. } => walk(inner),
                ExprKind::Literal { .. } | ExprKind::CharRange { .. } => false,
            }
        }

        self.ast
            .rules
            .iter()
            .flat_map(|r| r.alternatives.iter())
            .any(|a| walk(&a.body))
    }

    /// The flat expression rule (DESIGN.md §5.2).
    ///
    /// Note the absence of a postfix slot unless the table actually declares
    /// postfix operators: access (`call`, `index`, `field`) left the operator
    /// table for the grammar's own suffix chain (§6.7).
    fn emit_expr(&mut self, out: &mut String) {
        // A table with no operators produces no expressions worth folding, so
        // `expr` is emitted only if the grammar actually binds it. Otherwise a
        // grammar using `operators::none` paid for the whole driver — `expr`,
        // `ExprView`, precedence tables, `eval_tree` — all unreachable. That is
        // dead code in a file the user owns and cannot edit (DESIGN.md §11,
        // standing constraint 6), and the pest language server flags the rule.
        //
        // Keeping it when it *is* referenced matters: `value:expr` under
        // `operators::none` is a legitimate way to write a grammar that gains
        // operators later without rewriting every binding.
        if self.table.is_empty() && !self.references_expr() {
            return;
        }

        let Some(atom) = self.table.atom_rule.clone() else {
            return;
        };

        if !self.res.defined.contains_key(&atom) {
            let span = self
                .ast
                .precedence
                .first()
                .map(|b| b.span)
                .or_else(|| self.ast.uses.first().map(|u| u.span));
            let d = Diagnostic::error(format!(
                "the operator table's `atom` names `{atom}`, which is not defined"
            ))
            .help(format!("add `rule {atom} = ...;` for the operator driver to fold over"));
            self.diagnostics.push(match span {
                Some(s) => d.at(s),
                None => d,
            });
            return;
        }

        out.push_str("// Expressions (operator table is resolved at build time)\n");
        let atom = self.res.pest_name(&atom);

        let infix = self.emit_op_alternation(&[Fixity::Left, Fixity::Right], "nh_bin_op");
        let prefix = self.emit_op_alternation(&[Fixity::Prefix], "nh_pre_op");
        let postfix = self.emit_op_alternation(&[Fixity::Postfix], "nh_post_op");

        let pre = prefix.as_ref().map(|n| format!("{n}* ~ ")).unwrap_or_default();
        let post = postfix.as_ref().map(|n| format!(" ~ {n}*")).unwrap_or_default();
        let term = format!("{pre}{atom}{post}");

        let body = match &infix {
            Some(bin) => format!("{term} ~ ({bin} ~ {term})*"),
            None => term,
        };

        let _ = writeln!(out, "expr = {{ {body} }}\n");
        self.emitted_expr = true;
    }

    /// Emits one rule per operator plus a silent alternation over them.
    ///
    /// Returns `None` when the table has no operators of these fixities, so the
    /// caller can omit the slot entirely rather than emit an unmatchable rule.
    fn emit_op_alternation(&mut self, fixities: &[Fixity], group: &str) -> Option<String> {
        let mut ops: Vec<&nh_operators::Operator> = Vec::new();
        for f in fixities {
            ops.extend(self.table.sorted_by_fixity(*f));
        }
        if ops.is_empty() {
            return None;
        }

        // Re-sort across fixities: longest literal first (DESIGN.md §5.2).
        ops.sort_by(|a, b| {
            b.literal
                .len()
                .cmp(&a.literal.len())
                .then_with(|| a.literal.cmp(&b.literal))
        });

        let mut refs = Vec::new();
        let mut seen = HashSet::new();
        for op in ops {
            if !seen.insert(op.literal.clone()) {
                continue;
            }
            refs.push(self.operator_rule(op));
        }

        self.generated
            .push(format!("{group} = _{{ {} }}", refs.join(" | ")));
        Some(group.to_string())
    }

    fn operator_rule(&mut self, op: &nh_operators::Operator) -> String {
        if let Some(existing) = self.keyword_rules.get(&op.literal) {
            return existing.clone();
        }

        let name = self
            .alloc
            .alloc(&format!("{}op_{}", names::PREFIX, names::symbolic(&op.literal)));

        let body = if op.word {
            // A word operator is identifier-shaped, so it needs the same
            // boundary guard a keyword does — `ANDY` must not lex as `AND Y`.
            let token = self.ast.reserved.first().map(|r| r.token.value.clone());
            match token {
                Some(t) => {
                    let cont = self.continuation_rule(&t);
                    format!("{} ~ !{cont}", pest_syntax::string(&op.literal, true))
                }
                None => pest_syntax::string(&op.literal, true),
            }
        } else {
            pest_syntax::string(&op.literal, false)
        };

        self.generated.push(format!("{name} = @{{ {body} }}"));
        self.keyword_rules.insert(op.literal.clone(), name.clone());
        name
    }

    fn emit_rules(&mut self, out: &mut String) {
        let ast = self.ast;
        if ast.rules.is_empty() {
            return;
        }
        out.push_str("// Rules\n");

        for rule in &ast.rules {
            self.emit_rule(out, rule);
        }
    }

    fn emit_rule(&mut self, out: &mut String, rule: &RuleDef) {
        self.current_rule = Some(rule.name.value.clone());
        let mut alts = Vec::new();
        let mut subrules = Vec::new();
        let rule_name = self.res.pest_name(&rule.name.value);
        let mut body_name = rule_name.clone();

        // Error recovery lives in the *grammar*, not in runtime backtracking
        // (DESIGN.md §5.5), so the shape stays readable in the generated
        // `.pest`. The real body moves aside and the rule becomes a choice
        // between it and a node that swallows text up to a sync point.
        let recovery = self
            .ast
            .recovers
            .iter()
            .find(|r| r.rule.value == rule.name.value);

        if let Some(rec) = recovery {
            let outer = rule_name.clone();
            let ok = self.alloc.alloc(&names::ok(&outer));
            let err = self.alloc.alloc(&names::error(&outer));
            let sync = self.expr(&rec.sync, false);

            // `+` guarantees at least one character is consumed, so the error
            // node can never match empty — which would make `stmt*` spin
            // forever. The trailing sync is optional so trailing garbage with
            // no terminator still recovers instead of failing the parse.
            self.generated.push(format!(
                "{err} = {{ (!({sync}) ~ ANY)+ ~ ({sync})? }}"
            ));
            let _ = writeln!(out, "{outer} = {{ {ok} | {err} }}");

            self.recoveries.push(Recovery {
                rule: rule.name.value.clone(),
                error_rule: err,
            });
            // Only the *body* moves aside. Alternative sub-rules keep being
            // named after the user's rule, or every handler would be renamed to
            // `nh_ok_stmt_bind` by an unrelated recovery declaration.
            body_name = ok;
        }

        // A rule with exactly one labelled alternative needs no sub-rule: the
        // rule *is* the alternative. This avoids `entry_entry` and
        // `document_document`, and it means a single-alternative rule's
        // bindings are reachable through a view named after the rule.
        let single = recovery.is_none()
            && rule.alternatives.len() == 1
            && rule
                .alternatives
                .first()
                .and_then(|a| a.label.as_ref())
                .is_some_and(|l| l.value != "pass");

        if single {
            let alt = &rule.alternatives[0];
            let label = alt.label.as_ref().expect("checked above");
            self.current_label = Some(label.value.clone());
            let body = self.expr(&alt.body, false);
            self.current_label = None;
            self.record_alternative(rule, alt, label.value.clone(), rule_name.clone());
            self.rule_shapes.push(LoweredRule {
                name: rule.name.value.clone(),
                pest_rule: rule_name.clone(),
                shape: RuleShape::Single {
                    pest_rule: rule_name.clone(),
                },
                recovers: false,
            });
            let modifier = if rule.silent { "_" } else { "" };
            let _ = writeln!(out, "{rule_name} = {modifier}{{ {body} }}");
            self.current_rule = None;
            return;
        }

        let mut variants = Vec::new();

        for alt in &rule.alternatives {
            self.current_label = alt.label.as_ref().map(|l| l.value.clone());
            match &alt.label {
                // `-> pass` is transparent: no sub-rule, no handler.
                Some(l) if l.value == "pass" => {
                    alts.push(self.expr(&alt.body, false));
                    variants.push(LoweredVariant::Transparent {
                        child: self.sole_rule(&alt.body),
                    });
                }
                Some(label) => {
                    let sub = names::alternative(&rule_name, &label.value);
                    let body = self.expr(&alt.body, false);
                    self.record_alternative(rule, alt, label.value.clone(), sub.clone());
                    subrules.push(format!("{sub} = {{ {body} }}"));
                    variants.push(LoweredVariant::Labelled {
                        label: label.value.clone(),
                        pest_rule: sub.clone(),
                    });
                    alts.push(sub);
                }
                None => {
                    alts.push(self.expr(&alt.body, false));
                    variants.push(LoweredVariant::Transparent {
                        child: self.sole_rule(&alt.body),
                    });
                }
            }
            self.current_label = None;
        }

        // `rule atom = primary;` is one transparent alternative naming one
        // rule — an alias, not a choice. Distinguishing them matters because an
        // alias needs no type of its own.
        let shape = match variants.as_slice() {
            [LoweredVariant::Transparent { child }] => RuleShape::Alias {
                child: child.clone(),
            },
            _ => RuleShape::Choice(variants),
        };
        self.rule_shapes.push(LoweredRule {
            name: rule.name.value.clone(),
            pest_rule: rule_name.clone(),
            shape,
            recovers: recovery.is_some(),
        });

        // If any alternative became a sub-rule, this rule is a pass-through:
        // the pair it produces has one child, and that child carries the handler.
        if !subrules.is_empty() {
            self.wrapper_rules.push(body_name.clone());
        }

        let modifier = if rule.silent { "_" } else { "" };
        let _ = writeln!(out, "{body_name} = {modifier}{{ {} }}", alts.join(" | "));
        for sub in subrules {
            let _ = writeln!(out, "{sub}");
        }
        self.current_rule = None;
    }

    /// Rejects `lazy` where it cannot mean anything.
    fn check_lazy(&mut self, rule: &RuleDef, bindings: &[Binding], alt: &Alternative) {
        for b in bindings.iter().filter(|b| b.lazy) {
            if b.rule_ref.is_some() {
                continue;
            }
            self.diagnostics.push(
                Diagnostic::error(format!(
                    "`lazy` on `{}` has nothing to defer",
                    b.name
                ))
                .at(alt.span)
                .note(
                    "`lazy` defers evaluating a sub-rule; a token is already just \
                     text",
                    Some(rule.name.span),
                )
                .help("drop `lazy`, or bind a rule instead of a token"),
            );
        }
    }

    /// The single *rule* a transparent alternative yields, if there is one.
    ///
    /// `"(" inner:expr ")"` yields an `expr`; `primary` yields a `primary`. An
    /// alternative that names no rule, or more than one, yields `None` — the
    /// AST cannot give it a type, and generation says so rather than guessing.
    fn sole_rule(&self, e: &Expr) -> Option<String> {
        let mut found = None;
        let mut count = 0;
        self.walk_rules(e, &mut |name| {
            count += 1;
            if found.is_none() {
                found = Some(name.to_string());
            }
        });
        if count == 1 {
            found
        } else {
            None
        }
    }

    /// Visits every rule reference in an expression, skipping tokens and
    /// literals — they carry no node the AST would name.
    fn walk_rules(&self, e: &Expr, f: &mut impl FnMut(&str)) {
        match &e.kind {
            ExprKind::Ref(name) => match self.res.kind(name) {
                Some(crate::resolve::DefKind::Rule) => f(name),
                None if name == "expr" => f(name),
                _ => {}
            },
            ExprKind::Seq(parts) | ExprKind::Choice(parts) => {
                for p in parts {
                    self.walk_rules(p, f);
                }
            }
            ExprKind::Repeat { inner, .. } | ExprKind::Bind { inner, .. } => {
                self.walk_rules(inner, f)
            }
            // A lookahead consumes nothing, so it contributes no node.
            ExprKind::Lookahead { .. } => {}
            ExprKind::Literal { .. } | ExprKind::CharRange { .. } => {}
        }
    }

    fn record_alternative(
        &mut self,
        rule: &RuleDef,
        alt: &Alternative,
        label: String,
        pest_rule: String,
    ) {
        let mut bindings = Vec::new();
        collect_bindings(&alt.body, Cardinality::One, self.res, &mut bindings);
        self.check_lazy(rule, &bindings, alt);
        self.collected_alternatives.push(LoweredAlternative {
            rule: rule.name.value.clone(),
            label,
            pest_rule,
            source: nh_syntax::alternative_source(alt),
            bindings,
            place: alt.place,
        });
    }

    // -----------------------------------------------------------------------
    // Expressions
    // -----------------------------------------------------------------------

    /// Emits a pest fragment. `fold` forces literals to case-insensitive form,
    /// which is how a `case-insensitive` token propagates to its contents.
    fn expr(&mut self, e: &Expr, fold: bool) -> String {
        match &e.kind {
            ExprKind::Seq(parts) => parts
                .iter()
                .map(|p| {
                    let f = self.expr(p, fold);
                    if matches!(p.kind, ExprKind::Choice(_)) {
                        format!("({f})")
                    } else {
                        f
                    }
                })
                .collect::<Vec<_>>()
                .join(" ~ "),

            ExprKind::Choice(parts) => parts
                .iter()
                .map(|p| self.expr(p, fold))
                .collect::<Vec<_>>()
                .join(" | "),

            ExprKind::Repeat { inner, kind } => {
                let f = self.expr(inner, fold);
                let suffix = match kind {
                    RepeatKind::ZeroOrMore => "*",
                    RepeatKind::OneOrMore => "+",
                    RepeatKind::Optional => "?",
                };
                format!("{}{suffix}", pest_syntax::group(&f))
            }

            ExprKind::Lookahead { inner, negative } => {
                let f = self.expr(inner, fold);
                let prefix = if *negative { "!" } else { "&" };
                format!("{prefix}{}", pest_syntax::group(&f))
            }

            // A binding becomes a node tag. The tag must attach to a single
            // term, so anything compound is grouped first (DESIGN.md §2).
            //
            // When the binding wraps a repetition the tag goes *inside* it:
            // `(#name = x)*`, not `#name = x*`. Pest's grammar puts the postfix
            // operator inside the tagged term, and tagging a repetition does
            // not tag every iteration — the first one comes back untagged, so
            // `view.name()` silently drops the first element. Pushing the tag
            // inward tags each iteration, which is what a repeated binding
            // means.
            ExprKind::Bind { name, inner, .. } => match &inner.kind {
                ExprKind::Repeat {
                    inner: repeated,
                    kind,
                } => {
                    let body = self.expr(repeated, fold);
                    let suffix = match kind {
                        RepeatKind::ZeroOrMore => "*",
                        RepeatKind::OneOrMore => "+",
                        RepeatKind::Optional => "?",
                    };
                    format!(
                        "(#{} = {}){suffix}",
                        name.value,
                        pest_syntax::group(&body)
                    )
                }
                _ => {
                    let f = self.expr(inner, fold);
                    format!("#{} = {}", name.value, pest_syntax::group(&f))
                }
            },

            ExprKind::Literal {
                value,
                case_insensitive,
            } => {
                let ci = *case_insensitive || fold || self.fold_keywords;
                // A reserved keyword needs its boundary guard; an `expect`
                // literal needs its own rule so pest can name it. A keyword
                // wins, since the guard is a correctness matter.
                if let Some(rule) = self.keyword_for(value) {
                    return rule;
                }
                if let Some(rule) = self.expectation_for(value) {
                    return rule;
                }
                pest_syntax::string(value, ci)
            }

            ExprKind::CharRange { lo, hi } => match pest_syntax::char_range(lo, hi) {
                Some(r) => r,
                None => {
                    self.diagnostics.push(
                        Diagnostic::error("a character range needs single characters on both sides")
                            .at(e.span)
                            .help("write `\"a\"..\"z\"`, not a multi-character string"),
                    );
                    "ANY".to_string()
                }
            },

            ExprKind::Ref(name) => self.res.pest_name(name),
        }
    }

    /// If `literal` is a reserved word, returns the guarded rule that matches
    /// it, emitting that rule on first use.
    fn keyword_for(&mut self, literal: &str) -> Option<String> {
        let token = self
            .reserved_words
            .iter()
            .chain(self.guarded_words.iter())
            .find(|(_, words)| words.iter().any(|w| w == literal))
            .map(|(t, _)| t.clone())?;

        if let Some(existing) = self.keyword_rules.get(literal) {
            return Some(existing.clone());
        }

        let cont = self.continuation_rule(&token);
        let name = self
            .alloc
            .alloc(&format!("{}kw_{}", names::PREFIX, names::symbolic(literal)));
        let lit = pest_syntax::string(literal, self.fold_keywords);
        self.generated.push(format!("{name} = @{{ {lit} ~ !{cont} }}"));
        self.keyword_rules.insert(literal.to_string(), name.clone());
        Some(name)
    }
}

/// Walks an alternative body collecting bindings with their cardinality.
///
/// Cardinality is inherited from the enclosing structure: a binding under `*`
/// is `Many` however deeply nested, and a binding inside a multi-branch choice
/// is `Optional` because the other branch may match instead.
fn collect_bindings(e: &Expr, ctx: Cardinality, res: &Resolution, out: &mut Vec<Binding>) {
    match &e.kind {
        ExprKind::Bind { name, inner, lazy } => {
            // The repetition may be *inside* the binding: `args:arg_list?`
            // tags an optional node, so the accessor must be optional even
            // though nothing encloses the binding.
            out.push(Binding {
                name: name.value.clone(),
                cardinality: ctx.combine(own_cardinality(inner)),
                token: bound_token(inner, res),
                rule_ref: bound_rule(inner, res),
                lazy: *lazy,
            });
            // A binding inside a binding is still reachable, but its
            // cardinality is bounded by the outer one.
            collect_bindings(inner, ctx, res, out);
        }
        ExprKind::Seq(parts) => {
            for p in parts {
                collect_bindings(p, ctx, res, out);
            }
        }
        ExprKind::Choice(parts) => {
            let inner = if parts.len() > 1 {
                ctx.combine(Cardinality::Optional)
            } else {
                ctx
            };
            for p in parts {
                collect_bindings(p, inner, res, out);
            }
        }
        ExprKind::Repeat { inner, kind } => {
            let c = match kind {
                RepeatKind::Optional => ctx.combine(Cardinality::Optional),
                RepeatKind::ZeroOrMore | RepeatKind::OneOrMore => {
                    ctx.combine(Cardinality::Many)
                }
            };
            collect_bindings(inner, c, res, out);
        }
        // A lookahead consumes nothing, so nothing inside it can be bound to a
        // node that survives into the tree.
        ExprKind::Lookahead { .. } => {}
        _ => {}
    }
}

/// The cardinality implied by a bound expression's own outermost repetition.
fn own_cardinality(e: &Expr) -> Cardinality {
    match &e.kind {
        ExprKind::Repeat { kind, .. } => match kind {
            RepeatKind::Optional => Cardinality::Optional,
            RepeatKind::ZeroOrMore | RepeatKind::OneOrMore => Cardinality::Many,
        },
        _ => Cardinality::One,
    }
}

/// If a binding wraps exactly one *rule* reference, reports which rule.
fn bound_rule(e: &Expr, res: &Resolution) -> Option<String> {
    match &e.kind {
        ExprKind::Ref(name) => match res.kind(name) {
            Some(crate::resolve::DefKind::Rule) => Some(name.clone()),
            // `expr` comes from the operator table, not from a rule, but it is
            // dispatched exactly like one.
            None if name == "expr" => Some(name.clone()),
            _ => None,
        },
        ExprKind::Repeat { inner, .. } => bound_rule(inner, res),
        _ => None,
    }
}

/// If a binding wraps exactly one token reference, reports which token.
fn bound_token(e: &Expr, res: &Resolution) -> Option<BoundToken> {
    match &e.kind {
        ExprKind::Ref(name) => match res.kind(name) {
            Some(crate::resolve::DefKind::Token) => Some(BoundToken {
                name: name.clone(),
                case_insensitive: res.case_insensitive_tokens.contains(name),
            }),
            _ => None,
        },
        // `name:IDENT?` and `name:IDENT*` still bind the same token.
        ExprKind::Repeat { inner, .. } => bound_token(inner, res),
        _ => None,
    }
}
