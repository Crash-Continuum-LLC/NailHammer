//! The parse tree → owned AST conversion (M7).
//!
//! What matters here is that the result **owns** everything: after `build`, no
//! part of the program borrows the parse tree, which is what lets an
//! interpreter keep a piece of it for later.

use basic_interp::generated::ast;
use basic_interp::{BasicParser, Rule};
use nh_runtime::FileId;
use pest::Parser;
use std::rc::Rc;

fn build(source: &str) -> Rc<ast::Program> {
    let pair = BasicParser::parse(Rule::program, source)
        .unwrap_or_else(|e| panic!("{source}\n{e}"))
        .next()
        .expect("one program pair");
    ast::build_program(pair, FileId(0)).unwrap_or_else(|e| panic!("{source}\n{e}"))
}

#[test]
fn a_program_becomes_owned_lines() {
    let p = build("PRINT 1\nPRINT 2\n");
    assert_eq!(p.lines.len(), 2);
}

/// The whole point of M7: the tree outlives the parse. If any of it still
/// borrowed the input, this would not compile — the source is dropped first.
#[test]
fn the_tree_outlives_the_text_it_came_from() {
    let program = {
        let owned = String::from("PRINT 1\n");
        let pair = BasicParser::parse(Rule::program, &owned).unwrap().next().unwrap();
        ast::build_program(pair, FileId(0)).unwrap()
        // `owned` and the parse tree are both dropped here.
    };
    assert_eq!(program.lines.len(), 1);
}

/// A `lazy` body is now plain owned data, storable anywhere.
#[test]
fn a_loop_body_is_owned_and_shareable() {
    let p = build("FOR i = 1 TO 3\nPRINT i\nPRINT i\nNEXT i\n");
    let ast::Stmt::Loop(l) = &*p.lines[0].body else {
        panic!("expected a loop: {:?}", p.lines[0].body);
    };
    assert_eq!(l.body.len(), 2, "two lines in the body");
    assert_eq!(l.var.key(), "i");

    // Kept past the borrow it came from, which a `Deferred` could never be.
    let kept: Vec<Rc<ast::Line>> = l.body.clone();
    drop(p);
    assert_eq!(kept.len(), 2);
}

/// A folding token keeps both spellings.
#[test]
fn a_name_keeps_what_was_typed_and_what_to_look_up() {
    let p = build("Total = 1\n");
    let ast::Stmt::Let(l) = &*p.lines[0].body else {
        panic!("expected a let");
    };
    assert_eq!(l.target.text(), "Total");
    assert_eq!(l.target.key(), "total");
}

/// Operators are folded during the build, not on every evaluation. `2 + 3 * 4`
/// must come out as `2 + (3 * 4)`.
#[test]
fn expressions_are_folded_by_precedence_at_build_time() {
    let p = build("PRINT 2 + 3 * 4\n");
    let ast::Stmt::Print(pr) = &*p.lines[0].body else {
        panic!("expected a print");
    };
    let ast::Expr::Infix { op, rhs, .. } = &*pr.head else {
        panic!("expected an infix root: {:?}", pr.head);
    };
    assert_eq!(*op, Rule::nh_op_plus, "`+` is the looser operator, so it is the root");
    assert!(
        matches!(&**rhs, ast::Expr::Infix { op, .. } if *op == Rule::nh_op_star),
        "`*` binds tighter, so it is below: {rhs:?}"
    );
}

/// A `-> pass` alternative resolves to the type it yields, so a parenthesised
/// expression is an `Expr` and not a wrapper around one.
#[test]
fn a_parenthesised_expression_is_just_an_expression() {
    let p = build("PRINT (2 + 3) * 4\n");
    let ast::Stmt::Print(pr) = &*p.lines[0].body else {
        panic!("expected a print");
    };
    let ast::Expr::Infix { lhs, op, .. } = &*pr.head else {
        panic!("expected infix");
    };
    assert_eq!(*op, Rule::nh_op_star, "the parens made `*` the root");
    assert!(
        matches!(&**lhs, ast::Expr::Atom(a) if matches!(&**a, ast::Primary::Expr(_))),
        "the group is an atom holding an expression: {lhs:?}"
    );
}
