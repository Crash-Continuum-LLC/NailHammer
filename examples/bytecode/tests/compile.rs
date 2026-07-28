//! What a compiler host gets from NailHammer, checked end to end.
//!
//! These tests are the reason this example is shipped rather than kept as a
//! scratch experiment. DESIGN §4.1 claims one grammar drives an interpreter, a
//! bytecode emitter, and a typechecker. The first was demonstrated three times
//! over; the second was not, and the trait stack had quietly grown a
//! requirement — `truthy` — that only an interpreter can meet. These assertions
//! are what stops that happening again.

use bc::{generated, BcParser, Interp, Op, Rule};
use nh_runtime::{Ctx, SourceMap, Span};
use pest::Parser;

/// Compiles a program, returning the emitted instructions.
fn compile(source: &str) -> Vec<Op> {
    let mut sources = SourceMap::new();
    let file = sources.add("t.bc", source);
    let text = sources.text(file).to_string();

    let mut pairs = BcParser::parse(Rule::program, &text)
        .unwrap_or_else(|e| panic!("`{source}` did not parse:\n{e}"));
    let program = pairs.next().expect("one program pair");

    let mut cx = Ctx::new(sources);
    cx.enter(Span::new(file, 0, 0));
    let mut host = Interp::default();

    let tree = generated::ast::build_program(program, file)
        .unwrap_or_else(|e| panic!("`{source}`:\n{}", cx.render(&e)));
    generated::dispatch::eval_program(&mut host, &tree, &mut cx)
        .unwrap_or_else(|e| panic!("`{source}`:\n{}", cx.render(&e)));

    host.code
}

/// Compiles and then runs, so the instructions are checked against what they
/// actually compute rather than only against a hand-written expectation.
fn output(source: &str) -> Vec<String> {
    Interp {
        code: compile(source),
    }
    .run()
}

// ---------------------------------------------------------------------------
// Eager parameters give a compiler stack order for free
// ---------------------------------------------------------------------------

/// Handler parameters are evaluated left to right before the handler runs. For
/// a compiler, "evaluated" means "emitted" — so operand code lands before the
/// operator's instruction with no effort from the host at all.
#[test]
fn operands_are_emitted_before_their_operator() {
    assert_eq!(
        compile("1 + 2;"),
        vec![Op::Push(1.0), Op::Push(2.0), Op::Add, Op::Pop]
    );
}

/// Precedence is not something the compiler consults at emit time. It is
/// already in the *order* of the stream, put there by the operator driver.
#[test]
fn precedence_becomes_instruction_order() {
    assert_eq!(
        compile("2 + 3 * 4;"),
        vec![
            Op::Push(2.0),
            Op::Push(3.0),
            Op::Push(4.0),
            Op::Mul,
            Op::Add,
            Op::Pop,
        ]
    );
    assert_eq!(output("print 2 + 3 * 4;"), ["14"]);
}

#[test]
fn parentheses_reorder_the_stream() {
    assert_eq!(
        compile("(2 + 3) * 4;"),
        vec![
            Op::Push(2.0),
            Op::Push(3.0),
            Op::Add,
            Op::Push(4.0),
            Op::Mul,
            Op::Pop,
        ]
    );
}

/// Left associativity, visible as nesting in the instruction stream:
/// `(10 - 3) - 2`, not `10 - (3 - 2)`.
#[test]
fn associativity_survives_the_translation() {
    assert_eq!(output("print 10 - 3 - 2;"), ["5"]);
}

// ---------------------------------------------------------------------------
// `place` is a Store, `place_read` is a Load
// ---------------------------------------------------------------------------

/// The point of `place`: an assignment target must not be *read*. In an
/// interpreter that distinction avoids evaluating a subscript twice; here it is
/// the difference between emitting a Store and emitting a Load.
#[test]
fn an_assignment_target_is_stored_not_loaded() {
    assert_eq!(
        compile("x = 1;"),
        vec![Op::Push(1.0), Op::Store("x".into()), Op::Pop]
    );
    assert_eq!(
        compile("y = x;"),
        vec![Op::Load("x".into()), Op::Store("y".into()), Op::Pop]
    );
}

// ---------------------------------------------------------------------------
// `lazy` — the one thing this example could not do without
// ---------------------------------------------------------------------------

/// An interpreter reads `lazy` as "run this when I say". A compiler reads it as
/// "emit this where I say". Without it, the body would already be in the
/// stream before the handler could put a jump in front of it.
#[test]
fn a_lazy_body_is_emitted_where_the_handler_says() {
    assert_eq!(
        compile("if 1 then print 2;"),
        vec![
            Op::Push(1.0),
            Op::JumpIfFalse(4), // patched once the body's length was known
            Op::Push(2.0),
            Op::Print,
        ]
    );
}

/// The jump target is not guessable in advance, which is the whole reason the
/// body has to be emitted from inside the handler.
#[test]
fn the_jump_target_is_patched_to_the_real_end() {
    let code = compile("if 1 then print 1 + 2 + 3 + 4;");
    let Some(Op::JumpIfFalse(target)) = code.iter().find(|op| matches!(op, Op::JumpIfFalse(_)))
    else {
        panic!("no jump emitted:\n{code:#?}");
    };
    assert_eq!(*target, code.len(), "the jump should skip the whole body");
}

/// A compiler calls `.eval()` **once**, to emit a body that may run many times.
/// That is the opposite of an interpreter, which calls it once per execution —
/// and it is why a body appears exactly once in the stream.
#[test]
fn a_body_is_emitted_once_however_often_it_runs() {
    let code = compile("if 1 then print 7;");
    assert_eq!(code.iter().filter(|op| **op == Op::Print).count(), 1);
}

#[test]
fn a_false_condition_jumps_over_its_body() {
    assert_eq!(output("if 0 then print 999; print 1;"), ["1"]);
    assert_eq!(output("if 1 then print 999; print 1;"), ["999", "1"]);
}

// ---------------------------------------------------------------------------
// The shipped sample
// ---------------------------------------------------------------------------

#[test]
fn the_sample_compiles_and_runs() {
    let src = include_str!("../sample.bc");
    assert_eq!(output(src), ["28", "22", "5", "14", "111"]);
}
