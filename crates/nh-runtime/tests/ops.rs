//! Precedence-climbing tests, run against a real parse tree.
//!
//! `pest_vm` interprets a grammar at runtime, so these fold actual pairs rather
//! than a stand-in — which matters, because the builder's whole job is to
//! reshape what pest produces.

use nh_runtime::ops::{build, Assoc, Fixity, OpInfo, OpTree};
use pest_vm::Vm;

/// A flat expression grammar in the shape NailHammer emits.
const GRAMMAR: &str = r#"
WHITESPACE = _{ " " }
name  = @{ 'a'..'z' }
add   = @{ "+" }
sub   = @{ "-" }
mul   = @{ "*" }
pow   = @{ "^" }
neg   = @{ "~" }
fact  = @{ "!" }
pre   = _{ neg }
post  = _{ fact }
bin   = _{ add | sub | mul | pow }
expr  = { pre* ~ name ~ post* ~ (bin ~ pre* ~ name ~ post*)* }
"#;

fn info(rule: &str) -> Option<OpInfo> {
    let mk = |precedence, fixity, assoc| {
        Some(OpInfo {
            precedence,
            fixity,
            assoc,
        })
    };
    match rule {
        "add" | "sub" => mk(1, Fixity::Infix, Assoc::Left),
        "mul" => mk(2, Fixity::Infix, Assoc::Left),
        "pow" => mk(3, Fixity::Infix, Assoc::Right),
        "neg" => mk(4, Fixity::Prefix, Assoc::Right),
        "fact" => mk(5, Fixity::Postfix, Assoc::Left),
        _ => None,
    }
}

/// Folds `src` and renders the result fully parenthesised.
fn shape(src: &str) -> String {
    let (_, rules) = pest_meta::parse_and_optimize(GRAMMAR).expect("test grammar is valid");
    let vm = Vm::new(rules);
    let pairs = vm.parse("expr", src).unwrap_or_else(|e| panic!("{src}: {e}"));
    let expr = pairs.into_iter().next().expect("one expr pair");
    let parts: Vec<_> = expr.into_inner().collect();

    let tree = build(parts, info, info).unwrap_or_else(|e| panic!("{src}: {e}"));
    render(&tree)
}

fn render(t: &OpTree<'_, &str>) -> String {
    match t {
        OpTree::Atom(p) => p.as_str().trim().to_string(),
        OpTree::Prefix { op, operand } => format!("({}{})", op.as_str(), render(operand)),
        OpTree::Postfix { op, operand } => format!("({}{})", render(operand), op.as_str()),
        OpTree::Infix { op, lhs, rhs } => {
            format!("({} {} {})", render(lhs), op.as_str(), render(rhs))
        }
    }
}

#[test]
fn tighter_operators_bind_first() {
    assert_eq!(shape("a + b * c"), "(a + (b * c))");
    assert_eq!(shape("a * b + c"), "((a * b) + c)");
    assert_eq!(shape("a + b + c * d"), "((a + b) + (c * d))");
}

#[test]
fn left_associative_operators_group_leftward() {
    assert_eq!(shape("a - b - c"), "((a - b) - c)");
    assert_eq!(shape("a + b - c + d"), "(((a + b) - c) + d)");
}

#[test]
fn right_associative_operators_group_rightward() {
    assert_eq!(shape("a ^ b ^ c"), "(a ^ (b ^ c))");
    // Mixed: `^` is tighter than `*`, and still groups rightward.
    assert_eq!(shape("a * b ^ c ^ d"), "(a * (b ^ (c ^ d)))");
}

#[test]
fn prefix_operators_bind_their_operand_at_their_own_level() {
    // `~` is tighter than `*` here, so it takes only the atom.
    assert_eq!(shape("~a * b"), "((~a) * b)");
    assert_eq!(shape("~a ^ b"), "((~a) ^ b)");
    assert_eq!(shape("~~a"), "(~(~a))");
}

/// A prefix operator *looser* than an infix one absorbs the whole comparison.
/// This is BASIC's `NOT A = B` meaning `NOT (A = B)` — the opposite of C's `!`.
#[test]
fn a_loose_prefix_operator_absorbs_tighter_infix() {
    fn loose(rule: &str) -> Option<OpInfo> {
        match rule {
            // `neg` below `add`, unlike the table above.
            "neg" => Some(OpInfo {
                precedence: 1,
                fixity: Fixity::Prefix,
                assoc: Assoc::Right,
            }),
            "add" => Some(OpInfo {
                precedence: 2,
                fixity: Fixity::Infix,
                assoc: Assoc::Left,
            }),
            _ => None,
        }
    }

    let (_, rules) = pest_meta::parse_and_optimize(GRAMMAR).unwrap();
    let vm = Vm::new(rules);
    let pairs = vm.parse("expr", "~a + b").unwrap();
    let parts: Vec<_> = pairs.into_iter().next().unwrap().into_inner().collect();
    let tree = build(parts, loose, loose).unwrap();

    assert_eq!(render(&tree), "(~(a + b))");
}

#[test]
fn postfix_operators_apply_to_the_atom() {
    assert_eq!(shape("a! + b"), "((a!) + b)");
    assert_eq!(shape("a!!"), "((a!)!)");
}

#[test]
fn a_single_atom_needs_no_operators() {
    assert_eq!(shape("a"), "a");
}

// ---------------------------------------------------------------------------
// Malformed streams
//
// Error recovery can leave an error node inside an expression, and a recovery
// node consumes greedily — so the operand/operator stream reaching the builder
// is not guaranteed well-formed. It must fail, not panic.
// ---------------------------------------------------------------------------

/// Folds a hand-built sequence of rule names, bypassing the grammar so a
/// deliberately malformed stream can be constructed.
fn fold_raw(src: &str) -> Result<String, String> {
    let (_, rules) = pest_meta::parse_and_optimize(GRAMMAR).unwrap();
    let vm = Vm::new(rules);
    let pairs = vm.parse("expr", src).map_err(|e| e.to_string())?;
    let parts: Vec<_> = pairs.into_iter().next().unwrap().into_inner().collect();
    build(parts, info, info)
        .map(|t| render(&t))
        .map_err(|e| e.to_string())
}

#[test]
fn a_trailing_operator_is_an_error_not_a_panic() {
    // `a +` — the grammar will not produce this, but a recovery node splicing
    // into an expression can.
    let (_, rules) = pest_meta::parse_and_optimize(GRAMMAR).unwrap();
    let vm = Vm::new(rules);
    let pairs = vm.parse("expr", "a + b").unwrap();
    let mut parts: Vec<_> = pairs.into_iter().next().unwrap().into_inner().collect();
    parts.pop(); // drop the right-hand operand

    let err = build(parts, info, info).expect_err("a dangling operator must fail");
    assert_eq!(err.to_string(), "expected an operand");
}

#[test]
fn a_well_formed_stream_still_folds() {
    assert_eq!(fold_raw("a + b").unwrap(), "(a + b)");
}
