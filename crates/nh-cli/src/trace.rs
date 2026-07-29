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
    /// The handler calls that produce this argument.
    pub from: Vec<Node>,
    /// Operators the driver folds while producing it.
    pub operators: Vec<OpUse>,
}

pub struct OpUse {
    pub literal: String,
    pub role: String,
}

/// Everything `trace` needs to know about the grammar, indexed for lookup.
struct Index<'a> {
    /// pest rule name -> the alternative it stands for.
    alts: HashMap<&'a str, &'a LoweredAlternative>,
    /// literal -> the role it binds.
    ops: HashMap<String, String>,
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

        let mut ops = HashMap::new();
        for tier in &table.tiers {
            for op in &tier.operators {
                ops.insert(op.literal.clone(), op.role.clone());
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
    };
    let Some(a) = alt else { return node };

    // A repetition tags every element with the same name, so this is a list.
    let mut tagged: HashMap<String, Vec<Pair<'a>>> = HashMap::new();
    for c in pair.into_inner() {
        if let Some(t) = c.as_node_tag() {
            tagged.entry(t.to_string()).or_default().push(c);
        }
    }

    for p in params(a) {
        let mut arg = Arg {
            text: None,
            lazy: *index
                .lazy
                .get(&(a.pest_rule.as_str(), p.name.as_str()))
                .unwrap_or(&false),
            matched: false,
            from: Vec::new(),
            operators: Vec::new(),
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

/// Walks a tagged subtree, gathering the handler calls and operator uses that
/// produce one argument.
///
/// Untagged intermediates — `expr` and the operator scaffolding — are walked
/// *through* rather than reported, because no handler corresponds to them. The
/// operators found on the way belong to this argument, since that is where the
/// driver folds them.
fn collect(index: &Index<'_>, pair: Pair<'_>, into: &mut Arg) {
    let rule = pair.as_rule().to_string();
    if index.alts.contains_key(rule.as_str()) {
        into.from.push(build(index, pair));
        return;
    }
    // A statement `recover` got past. It routes nowhere — no handler runs for
    // it — and saying nothing would let it vanish from the trace entirely,
    // which is exactly the wrong impression to leave.
    if let Some(what) = index.recovered.get(rule.as_str()) {
        into.from.push(Node {
            handler: format!("<{what} did not parse>"),
            source: format!("recovered here; no handler runs for `{}`", pair.as_str().trim()),
            args: Vec::new(),
        });
        return;
    }
    for child in pair.into_inner() {
        if let Some(role) = index.ops.get(child.as_str().trim()) {
            into.operators.push(OpUse {
                literal: child.as_str().trim().to_string(),
                role: role.clone(),
            });
        }
        collect(index, child, into);
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
    if node.handler.starts_with('<') {
        out.push_str(&format!("{pad}{}\n", node.handler));
        out.push_str(&format!("{pad}  · {}\n", node.source));
        return;
    }
    out.push_str(&format!("{pad}{}  → {dir}/{}.rs\n", node.handler, node.handler));
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

        for op in &a.operators {
            out.push_str(&format!("{pad}    `{}` → Operators::{}\n", op.literal, op.role));
        }
        for child in &a.from {
            render_node(child, depth + 2, dir, out);
        }
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
            let ops: Vec<String> = a
                .operators
                .iter()
                .map(|o| {
                    format!(
                        "{{\"literal\":\"{}\",\"role\":\"{}\"}}",
                        esc(&o.literal),
                        esc(&o.role)
                    )
                })
                .collect();
            let from: Vec<String> = a.from.iter().map(to_json).collect();
            format!(
                "{{\"name\":\"{}\",\"ty\":\"{}\",\"text\":{text},\"lazy\":{},\"matched\":{},\"operators\":[{}],\"from\":[{}]}}",
                esc(&a.name),
                esc(&a.ty),
                a.lazy,
                a.matched,
                ops.join(","),
                from.join(",")
            )
        })
        .collect();

    format!(
        "{{\"handler\":\"{}\",\"source\":\"{}\",\"args\":[{}]}}",
        esc(&node.handler),
        esc(&node.source),
        args.join(",")
    )
}
