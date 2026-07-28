//! End-to-end tests for the operator driver.
//!
//! The grammar contains no operator handling whatsoever — precedence,
//! associativity, and short-circuiting all come from `use operators::core`.
//! These tests check that what comes out matches what `nh explain` promises.

use calc_interp::{generated, CalcParser, Interp, Rule, Value};
use nh_runtime::{Ctx, SourceMap, Span};
use pest::Parser;

/// Runs a program, returning the interpreter so side effects can be inspected.
fn run(source: &str) -> Result<Interp, String> {
    let mut sources = SourceMap::new();
    let file = sources.add("t.calc", source);
    let mut cx = Ctx::new(sources);
    let mut interp = Interp::default();

    match generated::eval_source(&mut interp, &mut cx, file) {
        Ok(_) => Ok(interp),
        Err(errors) => Err(errors
            .iter()
            .map(|d| d.render(cx.sources()))
            .collect::<Vec<_>>()
            .join("\n")),
    }
}

/// Evaluates a single expression.
fn eval(expr: &str) -> Value {
    let interp = run(&format!("{expr};")).unwrap_or_else(|e| panic!("`{expr}`:\n{e}"));
    let text = interp.output.last().expect("one result").clone();
    match text.parse::<f64>() {
        Ok(n) => Value::Num(n),
        Err(_) => Value::Bool(text == "true"),
    }
}

fn num(expr: &str) -> f64 {
    match eval(expr) {
        Value::Num(n) => n,
        other => panic!("`{expr}` gave {other}, expected a number"),
    }
}

// ---------------------------------------------------------------------------
// Precedence and associativity
// ---------------------------------------------------------------------------

#[test]
fn tighter_operators_bind_first() {
    assert_eq!(num("2 + 3 * 4"), 14.0, "not 20");
    assert_eq!(num("2 * 3 + 4"), 10.0);
    assert_eq!(num("(2 + 3) * 4"), 20.0, "parentheses still win");
}

#[test]
fn subtraction_is_left_associative() {
    assert_eq!(num("10 - 3 - 2"), 5.0, "(10-3)-2, not 10-(3-2)");
    assert_eq!(num("100 / 10 / 2"), 5.0);
}

#[test]
fn exponentiation_is_right_associative() {
    // 2^(3^2) = 2^9 = 512, not (2^3)^2 = 64.
    assert_eq!(num("2 ** 3 ** 2"), 512.0);
}

#[test]
fn a_custom_tier_lands_where_the_override_put_it() {
    // `right "**" above "*"` — tighter than `*`.
    assert_eq!(num("2 * 3 ** 2"), 18.0, "2 * (3**2), not (2*3)**2");
}

#[test]
fn prefix_binds_tighter_than_infix_here() {
    assert_eq!(num("-2 ** 2"), 4.0, "(-2)**2, not -(2**2)");
    assert_eq!(num("-2 + 5"), 3.0);
    assert_eq!(num("--3"), 3.0);
}

#[test]
fn comparison_is_looser_than_arithmetic() {
    // If `&&` bound tighter than `>`, this would parse as `(1 > 0 && 2) > 1`.
    assert_eq!(eval("1 + 1 > 1"), Value::Bool(true));
    assert_eq!(eval("1 > 0 && 2 > 1"), Value::Bool(true));
}

// ---------------------------------------------------------------------------
// Short-circuiting
//
// Proved by observation, not by asserting on intent: `trace(n)` records that it
// ran, so an operand that should never be evaluated leaves no trace.
// ---------------------------------------------------------------------------

#[test]
fn and_does_not_evaluate_its_right_operand_when_the_left_is_false() {
    let interp = run("false && trace(1);").unwrap();
    assert!(
        interp.traced.is_empty(),
        "`&&` evaluated its right operand: {:?}",
        interp.traced
    );
}

#[test]
fn and_does_evaluate_its_right_operand_when_the_left_is_true() {
    let interp = run("true && trace(1);").unwrap();
    assert_eq!(interp.traced, vec![1.0]);
}

#[test]
fn or_does_not_evaluate_its_right_operand_when_the_left_is_true() {
    let interp = run("true || trace(1);").unwrap();
    assert!(
        interp.traced.is_empty(),
        "`||` evaluated its right operand: {:?}",
        interp.traced
    );
}

#[test]
fn or_does_evaluate_its_right_operand_when_the_left_is_false() {
    let interp = run("false || trace(1);").unwrap();
    assert_eq!(interp.traced, vec![1.0]);
}

/// The interpreter never implements `and_then` or `or_else` — the generated
/// defaults do all of it, using only the `truthy` this language supplied.
#[test]
fn short_circuiting_guards_a_failing_operand() {
    // `trace(1/0)` would fail, but `&&` never reaches it.
    let interp = run("false && trace(1 / 0);").unwrap();
    assert!(interp.traced.is_empty());

    // Without the guard it does fail, which shows the guard was doing work.
    assert!(run("true && trace(1 / 0);").is_err());
}

// ---------------------------------------------------------------------------
// Everything else
// ---------------------------------------------------------------------------

#[test]
fn variables_round_trip() {
    let interp = run("let x = 6; let y = 7; x * y;").unwrap();
    assert_eq!(interp.output.last().unwrap(), "42");
}

#[test]
fn an_unimplemented_operator_reports_itself() {
    // `%` is in `core` but `rem` was never implemented, so it stays at its
    // generated default rather than silently doing something.
    let err = run("7 % 2;").unwrap_err();
    assert!(err.contains("not supported"), "{err}");
}

#[test]
fn runtime_errors_carry_a_location() {
    let err = run("let a = 1;\nb + 1;\n").unwrap_err();
    assert!(err.contains("undefined variable `b`"), "{err}");
    assert!(err.contains("t.calc:2:"), "expected a line-2 location:\n{err}");
}

#[test]
fn the_shipped_sample_evaluates() {
    let interp = run(include_str!("../sample.calc")).unwrap();
    // The two `if`s: the first body runs and `a` becomes 15, the second does
    // not run at all — which is only possible because `body` is `lazy`.
    assert_eq!(interp.output, vec!["14", "512", "4", "20", "true", "15", "15"]);
}

// ---------------------------------------------------------------------------
// Assignment and places (DESIGN.md §6.8)
// ---------------------------------------------------------------------------

#[test]
fn assignment_stores_into_a_variable() {
    let interp = run("let x = 1; x = 9; x;").unwrap();
    assert_eq!(interp.output.last().unwrap(), "9");
}

#[test]
fn assignment_stores_into_an_indexed_slot() {
    let interp = run("a[0] = 5; a[1] = 7; a[0] + a[1];").unwrap();
    assert_eq!(interp.output.last().unwrap(), "12");
}

/// `compound_assign` is never implemented by this interpreter. Its generated
/// default reads the place, applies `add`, and writes back — and `+=` is not
/// even in `operators::core`; one line in the grammar added the whole family.
#[test]
fn compound_assignment_works_without_being_implemented() {
    let interp = run("let x = 10; x += 5; x;").unwrap();
    assert_eq!(interp.output.last().unwrap(), "15");

    let interp = run("let y = 10; y -= 4; y;").unwrap();
    assert_eq!(interp.output.last().unwrap(), "6");
}

/// **The M3 acceptance criterion.**
///
/// `a[trace(0)] += 1` must evaluate the subscript exactly once. Compound
/// assignment reads the place and then writes it; if the place held an
/// unevaluated index rather than a value, `trace(0)` would run twice and
/// nothing would say so.
#[test]
fn a_subscript_with_a_side_effect_is_evaluated_exactly_once() {
    let interp = run("a[trace(0)] += 1;").unwrap();
    assert_eq!(
        interp.traced,
        vec![0.0],
        "the subscript must be evaluated once, not once per read/write"
    );
}

#[test]
fn compound_assignment_on_a_slot_accumulates() {
    let interp = run("a[2] = 1; a[2] += 4; a[2] += 5; a[2];").unwrap();
    assert_eq!(interp.output.last().unwrap(), "10");
}

/// An assignment target is resolved, never evaluated as a value — so assigning
/// to an undefined variable creates it rather than failing on a read.
#[test]
fn an_assignment_target_is_not_read_as_a_value() {
    let interp = run("fresh = 3; fresh;").unwrap();
    assert_eq!(interp.output.last().unwrap(), "3");
}

/// Compound assignment *does* read, so it needs the variable to exist.
#[test]
fn compound_assignment_reports_an_undefined_target() {
    let err = run("missing += 1;").unwrap_err();
    assert!(err.contains("undefined variable `missing`"), "{err}");
}

// ---------------------------------------------------------------------------
// Error recovery (DESIGN.md §5.5)
// ---------------------------------------------------------------------------

/// Parses and returns the recovered syntax errors, without evaluating.
fn syntax_errors(source: &str) -> Vec<String> {
    let mut sources = SourceMap::new();
    let file = sources.add("t.calc", source);
    let text = sources.text(file).to_string();

    let program = CalcParser::parse(Rule::program, &text)
        .unwrap_or_else(|e| panic!("recovery means this should still parse:\n{e}"))
        .next()
        .expect("one program pair");

    generated::syntax_errors(&program, file)
        .iter()
        .map(|d| d.render(&sources))
        .collect()
}

/// The point of recovery: a bad statement does not hide the ones after it.
#[test]
fn every_bad_statement_is_reported_not_just_the_first() {
    let errors = syntax_errors("let a = 1;\nlet b = @@@ ;\nlet c = 2;\nthis is junk;\nlet d = 3;\n");
    assert_eq!(errors.len(), 2, "both failures must be reported: {errors:#?}");
    assert!(errors[0].contains("t.calc:2:"), "{}", errors[0]);
    assert!(errors[1].contains("t.calc:4:"), "{}", errors[1]);
}

#[test]
fn a_clean_program_recovers_from_nothing() {
    assert!(syntax_errors("let a = 1; a;").is_empty());
}

/// Recovery must not swallow the statements around it.
#[test]
fn statements_after_a_syntax_error_still_evaluate() {
    let mut sources = SourceMap::new();
    let file = sources.add("t.calc", "let a = 2;\n@@@ ;\nlet b = a * 5;\nb;\n");
    let text = sources.text(file).to_string();

    let program = CalcParser::parse(Rule::program, &text)
        .unwrap()
        .next()
        .unwrap();
    assert_eq!(generated::syntax_errors(&program, file).len(), 1);

    let mut cx = Ctx::new(sources);
    cx.enter(Span::new(file, 0, 0));
    let mut interp = Interp::default();

    // Evaluation fails at the error node, but the good statements before it
    // ran — `a` was bound.
    let tree = generated::ast::build_program(program, file).unwrap();
    let _ = generated::dispatch::eval_program(&mut interp, &tree, &mut cx);
    assert_eq!(interp.vars.get("a"), Some(&Value::Num(2.0)));
}

/// An error node reports once and is *dropped from the list*, so a bad
/// statement costs exactly one message and does not stop the ones around it.
/// That is the whole return on `recover`: every statement that can run, runs.
#[test]
fn an_error_node_reports_once_and_the_rest_still_runs() {
    let mut sources = SourceMap::new();
    let file = sources.add("t.calc", "let a = 1; @@@ ; let b = 2;\n");
    let text = sources.text(file).to_string();

    let program = CalcParser::parse(Rule::program, &text)
        .unwrap()
        .next()
        .unwrap();

    let mut cx = Ctx::new(sources);
    cx.enter(Span::new(file, 0, 0));
    let mut interp = Interp::default();

    let tree = generated::ast::build_program(program, file).unwrap();
    generated::dispatch::eval_program(&mut interp, &tree, &mut cx)
        .expect("the good statements run");

    assert_eq!(cx.diagnostics().len(), 1, "exactly one message, not a cascade");
    assert!(cx.has_errors(), "the run is still a failure");
    // Both sides of the bad statement took effect.
    assert_eq!(interp.vars.get("a"), Some(&Value::Num(1.0)));
    assert_eq!(interp.vars.get("b"), Some(&Value::Num(2.0)));
}

/// ...but only *inside a repetition*. Evaluate an error node on its own and it
/// still poisons, so a handler that did not opt into salvaging cannot mistake
/// a failed node for a real value.
///
/// The node is built by hand here, which is only possible because the AST is
/// ordinary owned data — under the old pair-walking evaluator this test had to
/// dig a `nh_error_stmt` pair out of a real parse.
#[test]
fn a_lone_error_node_still_poisons() {
    let mut sources = SourceMap::new();
    let file = sources.add("t.calc", "@@@ ;\n");

    let mut cx = Ctx::new(sources);
    cx.enter(Span::new(file, 0, 0));
    let mut interp = Interp::default();

    let node = generated::ast::Stmt::Error(Span::new(file, 0, 5));
    let err = generated::dispatch::eval_stmt(&mut interp, &node, &mut cx).unwrap_err();

    assert_eq!(err, nh_runtime::Error::AlreadyReported);
    assert_eq!(cx.diagnostics().len(), 1, "reported exactly once");
}

/// `expect ")" in primary as "closing parenthesis"` gives the literal a rule
/// name and a sentence, so pest's expected-set can report something a user of
/// *this* language can act on.
#[test]
fn expect_labels_are_available_to_error_rendering() {
    assert_eq!(
        generated::diagnostics::describe(Rule::nh_expect_primary_rparen),
        Some("closing parenthesis")
    );
}

/// A consequence of recovery worth knowing: once the top-level statement rule
/// recovers, the parser essentially stops failing outright. Every failure
/// becomes an error node instead, so `render_parse_error` is for grammars
/// *without* recovery — and `syntax_errors` is the one that matters here.
#[test]
fn recovery_means_the_parse_itself_succeeds() {
    let source = "let x = (1 + 2";
    assert!(
        CalcParser::parse(Rule::program, source).is_ok(),
        "with `recover stmt`, unparseable input still yields a tree"
    );

    let errors = syntax_errors(source);
    assert_eq!(errors.len(), 1, "reported as a recovered error instead");
    assert!(errors[0].contains("could not parse this `stmt`"), "{}", errors[0]);
}

// ---------------------------------------------------------------------------
// `lazy` bindings
// ---------------------------------------------------------------------------

/// The point of `lazy`: the handler receives the body *unevaluated*, so an `if`
/// can decline to run it. Proved by observation — `trace` records that it ran.
#[test]
fn a_lazy_body_does_not_run_until_it_is_forced() {
    let interp = run("if false then trace(1);").unwrap();
    assert!(
        interp.traced.is_empty(),
        "the body ran anyway: {:?}",
        interp.traced
    );
}

/// ...and forcing it does run it, exactly once.
#[test]
fn forcing_a_lazy_body_runs_it_once() {
    let interp = run("if true then trace(7);").unwrap();
    assert_eq!(interp.traced, vec![7.0]);
}

/// A `lazy` binding defers the node, not the *whole* rule: `cond` is an
/// ordinary parameter and is evaluated before the handler is called.
#[test]
fn the_condition_is_still_evaluated_eagerly() {
    let interp = run("if trace(1) == 1 then trace(2);").unwrap();
    assert_eq!(interp.traced, vec![1.0, 2.0], "condition first, then body");
}

/// A language has one notion of truth. `if` must agree with `&&`, which means
/// the handler asks `Semantics::truthy` rather than testing for `false` itself
/// — this interpreter counts `0` as falsy, so the two are distinguishable.
#[test]
fn a_conditional_uses_the_languages_own_truthiness() {
    let interp = run("if 0 then trace(1);").unwrap();
    assert!(
        interp.traced.is_empty(),
        "`0` is falsy for `&&`, so it must be falsy for `if` too: {:?}",
        interp.traced
    );

    let short_circuit = run("0 && trace(1);").unwrap();
    assert_eq!(short_circuit.traced, interp.traced, "`if` and `&&` must agree");
}
