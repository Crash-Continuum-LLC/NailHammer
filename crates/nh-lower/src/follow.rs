//! What a recovered rule must not swallow.
//!
//! # The bug this exists to fix
//!
//! `recover stmt sync ";"` used to lower to
//!
//! ```text
//! nh_error_stmt = { (!(";") ~ ANY)+ ~ (";")? }
//! ```
//!
//! — consume anything that is not a `;`. Which includes the `}` that closes the
//! block the statement is inside:
//!
//! ```text
//! block = { "{" ~ (#stmts = stmt)* ~ "}" }
//! ```
//!
//! At the closing brace, `stmt`'s real body fails, the error node matches, and
//! it eats the brace and everything up to the next `;`. The repetition never
//! terminates and the block never closes. What the user sees is a parse error
//! pointing at the `if`, with nothing anywhere naming recovery.
//!
//! It fails for **every** grammar where a recovered rule appears inside a
//! delimited group, which is every grammar with blocks. The three examples in
//! this repository escaped only because they recover at the top level, where
//! the closer is `EOI` and `ANY` stops on its own.
//!
//! # The fix
//!
//! Recovery must stop at anything that could close an enclosing construct. That
//! set is not something a grammar author should have to write down — it is
//! derivable, and asking for it would be the boilerplate this project exists to
//! remove (DESIGN §0). So it is computed:
//!
//! ```text
//! nh_error_stmt = { (!(";") ~ !("}") ~ ANY)+ ~ (";")? }
//! ```
//!
//! # What is collected
//!
//! For a recovered rule `R`: every terminal that can appear immediately after
//! `R` — or after any rule that *contains* `R` — in some rule body.
//!
//! Transitivity is the part that is easy to get wrong. In the line-oriented
//! style the chain is three deep:
//!
//! ```text
//! recover stmt sync EOL;
//! rule block = stmts:line*;          // line contains stmt
//! rule line  = body:stmt EOL*;
//! rule stmt  = "WHILE" .. body:block "WEND" -> while;
//! ```
//!
//! `WEND` follows `block`, `block` contains `line`, `line` contains `stmt`. Stop
//! at any depth and the loop body eats its own terminator.

use std::collections::{BTreeSet, HashMap, HashSet};

use nh_syntax::ast::{Ast, Expr, ExprKind, RepeatKind, RuleDef};

/// A terminal that recovery must not consume, as written in the grammar.
///
/// Kept as the source literal rather than a lowered pest fragment so the caller
/// can spell it however it spells terminals — a bare literal, or the guarded
/// keyword rule if the word is reserved.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Stop {
    /// A literal like `}` or `WEND`.
    Literal(String),
    /// A named token or rule, like `EOL`.
    Ref(String),
}

/// Terminals that must end recovery for `rule`, in a stable order.
pub fn stops_for(ast: &Ast, rule: &str) -> Vec<Stop> {
    let contains = containment(ast);

    // Every rule whose body can lead to `rule`, plus `rule` itself. A terminal
    // following any of them would be eaten by recovery inside `rule`.
    let mut carriers: HashSet<&str> = HashSet::new();
    carriers.insert(rule);
    for (owner, inner) in &contains {
        if inner.contains(rule) {
            carriers.insert(owner.as_str());
        }
    }

    let rules: HashMap<&str, &RuleDef> =
        ast.rules.iter().map(|r| (r.name.value.as_str(), r)).collect();

    let mut stops = BTreeSet::new();
    for r in &ast.rules {
        for alt in &r.alternatives {
            collect(&alt.body, &carriers, &rules, &mut stops);
        }
    }
    stops.into_iter().collect()
}

/// `rule name -> every rule reachable from its body`.
fn containment(ast: &Ast) -> HashMap<String, HashSet<String>> {
    let mut direct: HashMap<String, HashSet<String>> = HashMap::new();
    for r in &ast.rules {
        let mut refs = HashSet::new();
        for alt in &r.alternatives {
            refs_in(&alt.body, &mut refs);
        }
        direct.insert(r.name.value.clone(), refs);
    }

    // Transitive closure. Grammars are small and this runs once per recovery,
    // so the naive fixpoint is the right amount of machinery.
    let mut changed = true;
    while changed {
        changed = false;
        let names: Vec<String> = direct.keys().cloned().collect();
        for name in names {
            let reach: Vec<String> = direct[&name].iter().cloned().collect();
            let mut extra = HashSet::new();
            for r in reach {
                if let Some(inner) = direct.get(&r) {
                    for x in inner {
                        if !direct[&name].contains(x) {
                            extra.insert(x.clone());
                        }
                    }
                }
            }
            if !extra.is_empty() {
                changed = true;
                direct.get_mut(&name).expect("present").extend(extra);
            }
        }
    }
    direct
}

fn refs_in(e: &Expr, out: &mut HashSet<String>) {
    match &e.kind {
        ExprKind::Ref(name) => {
            out.insert(name.clone());
        }
        ExprKind::Seq(items) | ExprKind::Choice(items) => {
            for i in items {
                refs_in(i, out);
            }
        }
        ExprKind::Repeat { inner, .. }
        | ExprKind::Lookahead { inner, .. }
        | ExprKind::Bind { inner, .. } => refs_in(inner, out),
        ExprKind::Literal { .. } | ExprKind::CharRange { .. } => {}
    }
}

/// Walks a body looking for `<something that carries the recovered rule>` and
/// takes the terminals that can start whatever comes next.
fn collect(
    e: &Expr,
    carriers: &HashSet<&str>,
    rules: &HashMap<&str, &RuleDef>,
    out: &mut BTreeSet<Stop>,
) {
    match &e.kind {
        ExprKind::Seq(items) => {
            for (i, item) in items.iter().enumerate() {
                if mentions_carrier(item, carriers) {
                    // Everything the following elements could begin with, up to
                    // and including the first that must match something.
                    for next in &items[i + 1..] {
                        let mandatory = leading(next, rules, &mut HashSet::new(), out);
                        if mandatory {
                            break;
                        }
                    }
                }
                collect(item, carriers, rules, out);
            }
        }
        ExprKind::Choice(items) => {
            for i in items {
                collect(i, carriers, rules, out);
            }
        }
        ExprKind::Repeat { inner, .. }
        | ExprKind::Lookahead { inner, .. }
        | ExprKind::Bind { inner, .. } => collect(inner, carriers, rules, out),
        ExprKind::Literal { .. } | ExprKind::CharRange { .. } | ExprKind::Ref(_) => {}
    }
}

fn mentions_carrier(e: &Expr, carriers: &HashSet<&str>) -> bool {
    match &e.kind {
        ExprKind::Ref(name) => carriers.contains(name.as_str()),
        ExprKind::Seq(items) | ExprKind::Choice(items) => {
            items.iter().any(|i| mentions_carrier(i, carriers))
        }
        ExprKind::Repeat { inner, .. }
        | ExprKind::Lookahead { inner, .. }
        | ExprKind::Bind { inner, .. } => mentions_carrier(inner, carriers),
        ExprKind::Literal { .. } | ExprKind::CharRange { .. } => false,
    }
}

/// Records what `e` can begin with. Returns whether `e` must match something —
/// if it can be skipped, the element after it can start the text too.
///
/// A reference to a *rule* expands to what that rule can start with, rather
/// than being emitted as a lookahead over the whole rule. `!("else")` says what
/// it means and costs one character; `!(else_tail)` would re-parse a block to
/// decide whether to stop.
fn leading(
    e: &Expr,
    rules: &HashMap<&str, &RuleDef>,
    seen: &mut HashSet<String>,
    out: &mut BTreeSet<Stop>,
) -> bool {
    match &e.kind {
        ExprKind::Literal { value, .. } => {
            out.insert(Stop::Literal(value.clone()));
            true
        }
        ExprKind::Ref(name) => {
            // `EOI`/`SOI` are positions rather than text; `ANY` would not stop
            // at one anyway, so excluding it would be noise.
            if name == "EOI" || name == "SOI" {
                return true;
            }
            match rules.get(name.as_str()) {
                // A token or builtin is already a terminal.
                None => {
                    out.insert(Stop::Ref(name.clone()));
                    true
                }
                // A rule stands for what it can begin with. `seen` stops a
                // left-recursive or mutually recursive grammar from spinning.
                Some(def) => {
                    if !seen.insert(name.clone()) {
                        return true;
                    }
                    let mut all_mandatory = !def.alternatives.is_empty();
                    for alt in &def.alternatives {
                        if !leading(&alt.body, rules, seen, out) {
                            all_mandatory = false;
                        }
                    }
                    all_mandatory
                }
            }
        }
        ExprKind::CharRange { .. } => true,
        ExprKind::Bind { inner, .. } => leading(inner, rules, seen, out),
        ExprKind::Repeat { inner, kind } => {
            let _ = leading(inner, rules, seen, out);
            // `*` and `?` can match nothing, so what follows them can also be
            // the first thing that appears.
            matches!(kind, RepeatKind::OneOrMore)
        }
        // A negative lookahead consumes nothing and stops nothing.
        ExprKind::Lookahead { .. } => false,
        ExprKind::Seq(items) => {
            for i in items {
                if leading(i, rules, seen, out) {
                    return true;
                }
            }
            false
        }
        ExprKind::Choice(items) => {
            let mut all_mandatory = !items.is_empty();
            for i in items {
                if !leading(i, rules, seen, out) {
                    all_mandatory = false;
                }
            }
            all_mandatory
        }
    }
}

