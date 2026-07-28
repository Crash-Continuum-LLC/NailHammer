//! The `.nh` abstract syntax tree.
//!
//! This is NailHammer's own internal AST for grammar files. It is unrelated to
//! the "no user-facing AST" decision in DESIGN.md §1 — that decision is about
//! what NailHammer generates for *target* languages, not about how it
//! represents grammars internally.
//!
//! Every node carries a [`Span`], because M0 already needs two-location
//! diagnostics for duplicate definitions across imports (§3.1).

use crate::source::{Span, Spanned};

/// A parsed `.nh` file, or the flat merge of several (see `import::resolve`).
#[derive(Debug, Default)]
pub struct Ast {
    pub grammar_name: Option<Spanned<String>>,
    pub imports: Vec<Import>,
    pub uses: Vec<UsePreset>,
    pub keywords_case: Option<Spanned<CaseMode>>,
    pub precedence: Vec<PrecedenceBlock>,
    pub skips: Vec<SkipDef>,
    pub tokens: Vec<TokenDef>,
    pub reserved: Vec<ReservedDef>,
    pub guards: Vec<GuardDef>,
    pub boundaries: Vec<BoundaryDef>,
    pub rules: Vec<RuleDef>,
    pub recovers: Vec<RecoverDef>,
    pub expects: Vec<ExpectDef>,
    pub allows: Vec<AllowDef>,
}

#[derive(Debug)]
pub struct Import {
    pub path: Spanned<String>,
    pub span: Span,
}

#[derive(Debug)]
pub struct UsePreset {
    pub preset: Spanned<String>,
    pub span: Span,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CaseMode {
    Insensitive,
    Sensitive,
}

#[derive(Debug)]
pub struct SkipDef {
    pub name: Spanned<String>,
    pub body: Expr,
    pub span: Span,
}

#[derive(Debug)]
pub struct TokenDef {
    pub name: Spanned<String>,
    /// `@` — atomic, producing no inner pairs.
    pub atomic: bool,
    /// Per-token half of the two case-folding knobs (DESIGN.md §5.3).
    pub case_insensitive: bool,
    pub body: Expr,
    pub span: Span,
}

#[derive(Debug)]
pub struct ReservedDef {
    /// The token these words are reserved *from*, e.g. `IDENT`.
    pub token: Spanned<String>,
    pub words: Vec<Spanned<String>>,
    pub span: Span,
}

/// `guard from IDENT { "atom" }` — boundary-guard without reserving.
///
/// Same shape as [`ReservedDef`], deliberately: the two differ only in whether
/// the identifier token is also taught to reject the words.
#[derive(Debug)]
pub struct GuardDef {
    pub token: Spanned<String>,
    pub words: Vec<Spanned<String>>,
    pub span: Span,
}

/// `boundary IDENT = ALNUM | "_";`
///
/// Overrides the derived continuation class for a token. The derivation reads
/// the operands of the token's repetitions, which is exactly right for an
/// ordinary identifier and an approximation for anything unusual.
#[derive(Debug)]
pub struct BoundaryDef {
    pub token: Spanned<String>,
    pub body: Expr,
    pub span: Span,
}

#[derive(Debug)]
pub struct RuleDef {
    pub name: Spanned<String>,
    /// `silent rule x = ...` — matches but produces no node.
    ///
    /// Pest rejects a node tag on a silent rule, so a binding inside one is an
    /// error rather than something the generated `.pest` discovers.
    pub silent: bool,
    pub alternatives: Vec<Alternative>,
    pub span: Span,
}

#[derive(Debug)]
pub struct Alternative {
    pub body: Expr,
    /// `-> label`. `None` means the alternative is unlabelled; `Some("pass")`
    /// is the transparent-passthrough label from DESIGN.md §3.
    pub label: Option<Spanned<String>>,
    /// `place` — assignable, and therefore a `Place` variant (§6.8). Only
    /// reachable after a label, which is what disambiguates the marker from a
    /// rule reference named `place`.
    pub place: bool,
    pub span: Span,
}

impl Alternative {
    /// Whether this alternative is transparent (`-> pass`).
    pub fn is_pass(&self) -> bool {
        self.label.as_deref().map(String::as_str) == Some("pass")
    }
}

#[derive(Debug)]
pub struct RecoverDef {
    pub rule: Spanned<String>,
    pub sync: Expr,
    pub span: Span,
}

#[derive(Debug)]
pub struct ExpectDef {
    pub literal: Spanned<String>,
    /// `rule` or `rule.label`.
    pub target: Vec<Spanned<String>>,
    pub message: Spanned<String>,
    pub span: Span,
}

/// `allow <lint> in <rule>;`
#[derive(Debug)]
pub struct AllowDef {
    pub lint: Spanned<String>,
    pub rule: Spanned<String>,
    pub span: Span,
}

// ---------------------------------------------------------------------------
// Expressions
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug)]
pub enum ExprKind {
    /// Juxtaposition: `a b c`.
    Seq(Vec<Expr>),
    /// Ordered choice: `a | b`. Order is significant and is never rewritten
    /// (DESIGN.md §5.2) — only synthesized operator alternations are sorted.
    Choice(Vec<Expr>),
    Repeat {
        inner: Box<Expr>,
        kind: RepeatKind,
    },
    Lookahead {
        inner: Box<Expr>,
        negative: bool,
    },
    /// `name:expr` — lowers to a pest node tag (DESIGN.md §2).
    Bind {
        name: Spanned<String>,
        inner: Box<Expr>,
        /// `lazy name:expr` — the handler receives it unevaluated.
        lazy: bool,
    },
    Literal {
        value: String,
        case_insensitive: bool,
    },
    CharRange {
        lo: String,
        hi: String,
    },
    /// A reference to a rule, token, or builtin. Which of the three it is gets
    /// resolved by `nh-analysis`, not here.
    Ref(String),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RepeatKind {
    ZeroOrMore,
    OneOrMore,
    Optional,
}

// ---------------------------------------------------------------------------
// Precedence
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct PrecedenceBlock {
    /// `precedence override { .. }` adjusts a preset instead of replacing it.
    pub is_override: bool,
    pub entries: Vec<PrecEntry>,
    pub span: Span,
}

#[derive(Debug)]
pub enum PrecEntry {
    /// `atom primary;` — names the rule the operator driver folds over.
    Atom {
        rule: Spanned<String>,
        span: Span,
    },
    /// `remove "," "->";`
    Remove {
        ops: Vec<OpRef>,
        span: Span,
    },
    Op(OpEntry),
}

#[derive(Debug)]
pub struct OpEntry {
    pub fixity: Spanned<Fixity>,
    pub ops: Vec<OpRef>,
    pub placement: Option<Placement>,
    /// `lazy(rhs)` — overrides the role's default laziness (DESIGN.md §6.6).
    pub lazy: Vec<Spanned<String>>,
    /// `-> role`. When absent, the spelling→role map supplies it (§6.3).
    pub role: Option<Spanned<String>>,
    pub span: Span,
}

#[derive(Debug)]
pub struct OpRef {
    /// `word "AND"` — identifier-shaped, so it needs a boundary guard and
    /// auto-reservation (DESIGN.md §6.5).
    pub word: bool,
    pub literal: Spanned<String>,
    pub span: Span,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Fixity {
    Left,
    Right,
    Prefix,
    Postfix,
}

#[derive(Debug)]
pub struct Placement {
    pub direction: Direction,
    pub anchor: Spanned<String>,
    pub span: Span,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Direction {
    Above,
    Below,
}
