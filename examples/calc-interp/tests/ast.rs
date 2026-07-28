//! The owned AST (M7), covering what `basic-interp`'s tests do not: a grammar
//! with error recovery, and one whose operator table has assignment and
//! short-circuiting.

use calc_interp::generated::ast;
use calc_interp::{CalcParser, Rule};
use nh_runtime::FileId;
use pest::Parser;
use std::rc::Rc;

fn build(source: &str) -> Rc<ast::Program> {
    let pair = CalcParser::parse(Rule::program, source)
        .unwrap_or_else(|e| panic!("{source}\n{e}"))
        .next()
        .expect("one program pair");
    ast::build_program(pair, FileId(0)).unwrap_or_else(|e| panic!("{source}\n{e}"))
}

/// A statement the parser recovered from has no shape to build, so it becomes
/// an `Error` node carrying where it was. The lines around it are unaffected —
/// that is the whole return on `recover`.
#[test]
fn a_recovered_statement_becomes_an_error_node() {
    let p = build("let a = 1;\n@@@ ;\nlet b = 2;\n");
    assert_eq!(p.stmts.len(), 3);

    assert!(matches!(&*p.stmts[0], ast::Stmt::Bind(_)), "{:?}", p.stmts[0]);
    assert!(matches!(&*p.stmts[1], ast::Stmt::Error(_)), "{:?}", p.stmts[1]);
    assert!(matches!(&*p.stmts[2], ast::Stmt::Bind(_)), "{:?}", p.stmts[2]);
}

/// The error node knows where it was, so the diagnostic can still point at it.
#[test]
fn an_error_node_keeps_its_span() {
    let p = build("@@@ ;\n");
    let ast::Stmt::Error(span) = &*p.stmts[0] else {
        panic!("expected an error node");
    };
    assert!(span.hi > span.lo, "an empty span would point nowhere");
}

/// `**` is right-associative, and the fold that decides so happens once, here.
#[test]
fn associativity_is_settled_when_the_tree_is_built() {
    let p = build("2 ** 3 ** 2;\n");
    let ast::Stmt::Eval(e) = &*p.stmts[0] else {
        panic!("expected an eval");
    };
    let ast::Expr::Infix { rhs, .. } = &*e.value else {
        panic!("expected infix");
    };
    assert!(
        matches!(&**rhs, ast::Expr::Infix { op, .. } if *op == Rule::nh_op_star_star),
        "right-associative means the second `**` nests to the right: {rhs:?}"
    );
}

/// A `lazy` binding is owned data now, so an `if` body can be kept.
#[test]
fn a_lazy_body_is_owned() {
    let p = build("if 1 then 2;\n");
    let ast::Stmt::Iff(i) = &*p.stmts[0] else {
        panic!("expected an if: {:?}", p.stmts[0]);
    };
    let kept: Rc<ast::Stmt> = i.body.clone();
    drop(p);
    assert!(matches!(&*kept, ast::Stmt::Eval(_)));
}
