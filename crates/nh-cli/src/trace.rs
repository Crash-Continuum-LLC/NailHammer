//! `nh trace` — what a program in *your* language routes to, and with what.
//!
//! # Why this can exist
//!
//! Answering "which handler gets this, and what does it receive?" normally
//! means generating Rust, compiling it, and adding print statements — a
//! minute's round trip to learn something the grammar already determines.
//!
//! It does not have to. `pest_vm` interprets a grammar at run time, so `nh` can
//! lower your `.nh`, parse a sample program against it, and read the node tags
//! back out. **Nothing is compiled.** The answer takes as long as parsing.
//!
//! # What it shows
//!
//! One line per handler that would be called, nested the way the calls nest,
//! and under each the parameters it would receive — with the token text where
//! the parameter is a token, and a note where it is the result of evaluating
//! something below.
//!
//! `lazy` parameters are marked, because that is the one case where the thing
//! below is *not* evaluated before the call — and it is the thing people most
//! often get wrong.

use std::collections::HashMap;

use nh_codegen::params::params;
use nh_lower::{Lowered, LoweredAlternative};
use nh_operators::OperatorTable;

/// One handler call.
///
/// Children hang off the **argument they produce**, not off the node, because
/// "what gets passed" is the question this exists to answer. A flat child list
/// would show `if x > 1 { .. } else { .. }` as five siblings and leave you to
/// work out which two were the condition.
pub struct Node {
    pub handler: String,
    /// The alternative as written in the grammar.
    pub source: String,
    pub args: Vec<Arg>,
    /// Handlers reached through parts of this rule that no binding names.
    ///
    /// Not every rule binds everything it matches — `rule program = SOI line+
    /// EOI;` binds nothing at all. Walking only the bindings dropped the whole
    /// tree beneath such a rule and showed a bare `program` with nothing in it.
    pub inside: Vec<Node>,
    pub kind: Kind,
}

#[derive(PartialEq, Clone, Copy)]
pub enum Kind {
    /// A labelled alternative — a file in `handlers/`.
    Handler,
    /// An operator the driver folds. There is no handler for one; it goes to
    /// `Operators::<role>`.
    Operator,
    /// Text a `recover` got past. It routes nowhere.
    Unparsed,
    /// A rule with no `-> label`, so no handler is generated for it. Its
    /// contents still route somewhere, which is why it is shown rather than
    /// skipped.
    Pass,
}

impl Arg {
    fn empty() -> Self {
        Arg {
            name: String::new(),
            ty: String::new(),
            text: None,
            lazy: false,
            matched: false,
            from: Vec::new(),
        }
    }
}

pub struct Arg {
    pub name: String,
    pub ty: String,
    /// The token's text, when the parameter is a token.
    pub text: Option<String>,
    pub lazy: bool,
    /// Whether anything in the program matched this binding at all. `false` for
    /// an `x?` that was not there, which is a different thing from an argument
    /// that matched and produced nothing.
    pub matched: bool,
    /// What produces this argument — handler calls and operator applications,
    /// nested exactly as the driver nests them.
    pub from: Vec<Node>,
}

/// What the fold needs to know about one operator.
#[derive(Clone)]
struct OpInfo {
    role: String,
    /// Tier index. Higher binds tighter. This is the binding power the fold
    /// below climbs with, so it must stay the raw index.
    prec: usize,
    /// The number to *print*, which is the inverse of `prec` — see
    /// `OperatorTable::display_precedence`. Kept separately rather than
    /// recomputed, because trace has no table by the time it renders, and
    /// recomputing is how the two commands disagreed in the first place.
    shown_prec: usize,
    fixity: nh_syntax::ast::Fixity,
    /// Operand positions the driver leaves unevaluated.
    lazy: Vec<String>,
}

/// Everything `trace` needs to know about the grammar, indexed for lookup.
struct Index<'a> {
    /// pest rule name -> the alternative it stands for.
    alts: HashMap<&'a str, &'a LoweredAlternative>,
    /// literal -> every reading of it.
    ///
    /// A `Vec`, because a literal can be two operators: `-` is prefix negation
    /// *and* infix subtraction. Keying by literal alone kept whichever was
    /// declared last, which silently lost every infix `-`.
    ops: HashMap<String, Vec<OpInfo>>,
    /// Which bindings are `lazy`.
    lazy: HashMap<(&'a str, &'a str), bool>,
    /// The rules a failed parse is recovered into, and what they stand for.
    recovered: HashMap<&'a str, &'a str>,
}

impl<'a> Index<'a> {
    fn new(lowered: &'a Lowered, table: &OperatorTable) -> Self {
        let mut alts = HashMap::new();
        let mut lazy = HashMap::new();
        for a in &lowered.alternatives {
            alts.insert(a.pest_rule.as_str(), a);
            for b in &a.bindings {
                lazy.insert((a.pest_rule.as_str(), b.name.as_str()), b.lazy);
            }
        }

        // Tier 0 binds loosest, so the index *is* the precedence.
        let mut ops = HashMap::new();
        for (prec, tier) in table.tiers.iter().enumerate() {
            for op in &tier.operators {
                ops.entry(op.literal.clone()).or_insert_with(Vec::new).push(OpInfo {
                    role: tier.grouped_role.clone().unwrap_or_else(|| op.role.clone()),
                    prec,
                    shown_prec: table.display_precedence(prec),
                    fixity: tier.fixity,
                    lazy: op.lazy.clone(),
                });
            }
        }

        let recovered = lowered
            .recoveries
            .iter()
            .map(|r| (r.error_rule.as_str(), r.rule.as_str()))
            .collect();

        Index { alts, ops, lazy, recovered }
    }
}

/// Builds the trace for `source`, parsed with `entry`.
pub fn trace(
    lowered: &Lowered,
    table: &OperatorTable,
    entry: &str,
    source: &str,
) -> Result<Node, String> {
    let vm = pest_vm::Vm::new(
        pest_meta::parse_and_optimize(&lowered.pest)
            .map_err(|e| format!("the grammar did not lower to valid pest: {e:?}"))?
            .1,
    );
    let pairs = vm.parse(entry, source).map_err(|e| e.to_string())?;
    let index = Index::new(lowered, table);

    let root = pairs
        .into_iter()
        .next()
        .ok_or("the entry rule matched nothing")?;
    Ok(build(&index, root))
}

type Pair<'a> = pest::iterators::Pair<'a, &'a str>;

fn build<'a>(index: &Index<'_>, pair: Pair<'a>) -> Node {
    let rule = pair.as_rule().to_string();
    let alt = index.alts.get(rule.as_str()).copied();

    let mut node = Node {
        handler: alt.map(|a| a.pest_rule.clone()).unwrap_or(rule),
        source: alt.map(|a| a.source.clone()).unwrap_or_default(),
        args: Vec::new(),
        inside: Vec::new(),
        kind: Kind::Handler,
    };
    let Some(a) = alt else {
        // A rule with no `-> label` generates no handler. Its contents still
        // route somewhere, so walk through and report what they reach.
        node.kind = Kind::Pass;
        node.source = "no `-> label`, so no handler is generated".into();
        let mut arg = Arg::empty();
        for c in pair.into_inner() {
            collect(index, c, &mut arg);
        }
        node.inside = arg.from;
        return node;
    };

    // A repetition tags every element with the same name, so this is a list.
    let mut tagged: HashMap<String, Vec<Pair<'a>>> = HashMap::new();
    let mut untagged: Vec<Pair<'a>> = Vec::new();
    for c in pair.into_inner() {
        match c.as_node_tag() {
            Some(t) => tagged.entry(t.to_string()).or_default().push(c),
            None => untagged.push(c),
        }
    }

    // Anything a binding does not name still evaluates, so it is walked too —
    // just reported as `inside` rather than as an argument.
    let named: Vec<String> = a.bindings.iter().map(|b| b.name.clone()).collect();
    let mut loose = Arg::empty();
    for c in untagged {
        collect(index, c, &mut loose);
    }
    node.inside = loose.from;
    let _ = named;

    for p in params(a) {
        let mut arg = Arg {
            text: None,
            lazy: *index
                .lazy
                .get(&(a.pest_rule.as_str(), p.name.as_str()))
                .unwrap_or(&false),
            matched: false,
            from: Vec::new(),
            name: p.name.clone(),
            ty: p.ty.clone(),
        };

        let is_token = p.ty.contains("str") || p.ty.contains("Name");
        for c in tagged.get(&p.name).into_iter().flatten() {
            arg.matched = true;
            if is_token {
                arg.text = Some(c.as_str().to_string());
            } else {
                collect(index, c.clone(), &mut arg);
            }
        }
        node.args.push(arg);
    }
    node
}

/// One item in the flat sequence pest produces for an expression.
enum Tok {
    Atom(Node),
    Op(String, OpInfo),
}

/// Walks a tagged subtree, gathering what produces one argument.
///
/// Untagged intermediates — `atom`, `primary`, the operator scaffolding — are
/// walked *through*, because no handler corresponds to them.
fn collect(index: &Index<'_>, pair: Pair<'_>, into: &mut Arg) {
    let rule = pair.as_rule().to_string();
    if index.alts.contains_key(rule.as_str()) {
        into.from.push(build(index, pair));
        return;
    }
    // Text a `recover` got past. It routes nowhere — no handler runs for it —
    // and saying nothing would let it vanish from the trace entirely, which is
    // exactly the wrong impression to leave.
    if let Some(what) = index.recovered.get(rule.as_str()) {
        into.from.push(Node {
            handler: format!("<{what} did not parse>"),
            source: format!("recovered here; no handler runs for `{}`", pair.as_str().trim()),
            args: Vec::new(),
            inside: Vec::new(),
            kind: Kind::Unparsed,
        });
        return;
    }
    if rule == "expr" {
        into.from.push(fold_expr(index, pair));
        return;
    }
    for child in pair.into_inner() {
        collect(index, child, into);
    }
}

/// Folds one `expr` the way the generated driver folds it.
///
/// pest hands back a flat sequence — `2 · + · 3 · * · 4` — because the grammar
/// deliberately has no precedence ladder in it (DESIGN §5.2). Precedence lives
/// in the table, and the driver applies it at run time. Showing the flat list
/// would answer "which roles are involved" but not the question people actually
/// have, which is **in what order**.
fn fold_expr(index: &Index<'_>, pair: Pair<'_>) -> Node {
    let mut toks = Vec::new();
    scan(index, pair, &mut toks);
    let mut pos = 0;
    parse_expr(&toks, &mut pos, 0).unwrap_or_else(|| Node {
        handler: "<empty expression>".into(),
        source: String::new(),
        args: Vec::new(),
        inside: Vec::new(),
        kind: Kind::Unparsed,
    })
}

/// Flattens an `expr` into atoms and operators.
///
/// A nested `expr` is an atom, not more tokens — that is what parentheses are,
/// and flattening through them would fold `(2 + 3) * 4` as `2 + (3 * 4)`.
fn scan(index: &Index<'_>, pair: Pair<'_>, out: &mut Vec<Tok>) {
    for child in pair.into_inner() {
        let rule = child.as_rule().to_string();
        if rule == "expr" {
            out.push(Tok::Atom(fold_expr(index, child)));
            continue;
        }
        if index.alts.contains_key(rule.as_str()) {
            out.push(Tok::Atom(build(index, child)));
            continue;
        }
        if let Some(readings) = index.ops.get(child.as_str().trim()) {
            // Which reading depends on position, exactly as it does when you
            // read the source: after an operand `-` subtracts, otherwise it
            // negates.
            let after_operand = matches!(out.last(), Some(Tok::Atom(_)));
            let pick = readings
                .iter()
                .find(|i| infix_like(i.fixity) == after_operand)
                .or_else(|| readings.first());
            if let Some(info) = pick {
                out.push(Tok::Op(child.as_str().trim().to_string(), info.clone()));
                continue;
            }
        }
        scan(index, child, out);
    }
}

/// Precedence climbing, with the table's tiers as the precedence and the tier's
/// fixity as the associativity.
fn parse_expr(toks: &[Tok], pos: &mut usize, min_prec: usize) -> Option<Node> {
    let mut lhs = parse_unary(toks, pos)?;

    while let Some(Tok::Op(lit, info)) = toks.get(*pos) {
        use nh_syntax::ast::Fixity;
        if !matches!(info.fixity, Fixity::Left | Fixity::Right) || info.prec < min_prec {
            break;
        }
        *pos += 1;
        // Left-associative: the right side must bind *tighter* to stay right.
        let next = if info.fixity == Fixity::Left {
            info.prec + 1
        } else {
            info.prec
        };
        let rhs = parse_expr(toks, pos, next)?;
        lhs = binary(lit, info, lhs, rhs);
    }
    Some(lhs)
}

fn parse_unary(toks: &[Tok], pos: &mut usize) -> Option<Node> {
    use nh_syntax::ast::Fixity;
    let mut prefixes = Vec::new();
    while let Some(Tok::Op(lit, info)) = toks.get(*pos) {
        if info.fixity != Fixity::Prefix {
            break;
        }
        prefixes.push((lit.clone(), info.clone()));
        *pos += 1;
    }

    let mut node = match toks.get(*pos) {
        Some(Tok::Atom(_)) => {
            let Some(Tok::Atom(n)) = toks.get(*pos) else { unreachable!() };
            *pos += 1;
            clone_node(n)
        }
        _ => return None,
    };

    // Applied outermost-last: `- - x` negates once, then again.
    for (lit, info) in prefixes.into_iter().rev() {
        node = unary(&lit, &info, node);
    }
    Some(node)
}

fn binary(lit: &str, info: &OpInfo, lhs: Node, rhs: Node) -> Node {
    let lazy_rhs = info.lazy.iter().any(|l| l == "rhs");
    Node {
        handler: format!("Operators::{}", info.role),
        source: format!("`{lit}` — {}", how(info)),
        kind: Kind::Operator,
        inside: Vec::new(),
        args: vec![
            Arg {
                name: "lhs".into(),
                ty: "Self::Out".into(),
                text: None,
                lazy: info.lazy.iter().any(|l| l == "lhs"),
                matched: true,
                from: vec![lhs],
            },
            Arg {
                name: "rhs".into(),
                // A lazy operand arrives as the node, which is what makes
                // `&&` able to not evaluate it at all.
                ty: if lazy_rhs { "Shared<Expr>".into() } else { "Self::Out".into() },
                text: None,
                lazy: lazy_rhs,
                matched: true,
                from: vec![rhs],
            },
        ],
    }
}

fn unary(lit: &str, info: &OpInfo, operand: Node) -> Node {
    Node {
        handler: format!("Operators::{}", info.role),
        source: format!("`{lit}` — {}", how(info)),
        kind: Kind::Operator,
        inside: Vec::new(),
        args: vec![Arg {
            name: "operand".into(),
            ty: "Self::Out".into(),
            text: None,
            lazy: false,
            matched: true,
            from: vec![operand],
        }],
    }
}

fn infix_like(f: nh_syntax::ast::Fixity) -> bool {
    use nh_syntax::ast::Fixity;
    matches!(f, Fixity::Left | Fixity::Right | Fixity::Postfix)
}

fn how(info: &OpInfo) -> String {
    use nh_syntax::ast::Fixity;
    let f = match info.fixity {
        Fixity::Left => "left-associative",
        Fixity::Right => "right-associative",
        Fixity::Prefix => "prefix",
        Fixity::Postfix => "postfix",
    };
    format!("{f}, precedence {}", info.shown_prec)
}

/// `Node` is a tree of owned strings; the fold needs to move an atom out of a
/// borrowed slice, so it takes a copy rather than complicating the scan.
fn clone_node(n: &Node) -> Node {
    Node {
        handler: n.handler.clone(),
        source: n.source.clone(),
        kind: n.kind,
        inside: n.inside.iter().map(clone_node).collect(),
        args: n
            .args
            .iter()
            .map(|a| Arg {
                name: a.name.clone(),
                ty: a.ty.clone(),
                text: a.text.clone(),
                lazy: a.lazy,
                matched: a.matched,
                from: a.from.iter().map(clone_node).collect(),
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

pub fn render(node: &Node, handlers_dir: &str) -> String {
    let mut out = String::new();
    render_node(node, 0, handlers_dir, &mut out);
    out
}

fn render_node(node: &Node, depth: usize, dir: &str, out: &mut String) {
    let pad = "  ".repeat(depth);
    match node.kind {
        Kind::Unparsed => {
            out.push_str(&format!("{pad}{}\n", node.handler));
            out.push_str(&format!("{pad}  · {}\n", node.source));
            return;
        }
        // No file: there is no handler for either of these.
        Kind::Operator => out.push_str(&format!("{pad}{}\n", node.handler)),
        Kind::Pass => out.push_str(&format!("{pad}{}\n", node.handler)),
        Kind::Handler => {
            out.push_str(&format!("{pad}{}  → {dir}/{}.rs\n", node.handler, node.handler))
        }
    }
    if !node.source.is_empty() {
        out.push_str(&format!("{pad}  · {}\n", node.source.trim()));
    }

    for a in &node.args {
        let head = format!("{pad}  {}: {}", a.name, a.ty);
        match (&a.text, a.lazy, a.matched) {
            // An optional binding the program did not use.
            (_, _, false) => out.push_str(&format!("{head}   ⟵ absent here\n")),
            (Some(t), _, _) => out.push_str(&format!("{head} = {t:?}\n")),
            // The one case where what is below has *not* run before the call.
            (None, true, _) => out.push_str(&format!("{head}   ⟵ lazy: the node, unevaluated\n")),
            (None, false, _) => out.push_str(&format!("{head}   ⟵ evaluated first, by:\n")),
        }

        for child in &a.from {
            render_node(child, depth + 2, dir, out);
        }
    }

    for child in &node.inside {
        render_node(child, depth + 1, dir, out);
    }
}

/// The same tree as JSON, for the editor extension.
pub fn to_json(node: &Node) -> String {
    fn esc(s: &str) -> String {
        let q = crate::json::quote(s);
        q[1..q.len() - 1].to_string()
    }
    let args: Vec<String> = node
        .args
        .iter()
        .map(|a| {
            let text = match &a.text {
                Some(t) => crate::json::quote(t),
                None => "null".to_string(),
            };
            let from: Vec<String> = a.from.iter().map(to_json).collect();
            format!(
                "{{\"name\":\"{}\",\"ty\":\"{}\",\"text\":{text},\"lazy\":{},\"matched\":{},\"from\":[{}]}}",
                esc(&a.name),
                esc(&a.ty),
                a.lazy,
                a.matched,
                from.join(",")
            )
        })
        .collect();

    format!(
        "{{\"kind\":\"{}\",\"handler\":\"{}\",\"source\":\"{}\",\"args\":[{}],\"inside\":[{}]}}",
        match node.kind {
            Kind::Handler => "handler",
            Kind::Operator => "operator",
            Kind::Unparsed => "unparsed",
            Kind::Pass => "pass",
        },
        esc(&node.handler),
        esc(&node.source),
        args.join(","),
        node.inside.iter().map(to_json).collect::<Vec<_>>().join(",")
    )
}
