//! What Pebble *does*, tested through the front door.
//!
//! Every test here runs a whole program and checks what it printed. That is the
//! level a language's behaviour actually lives at: a test that reached inside
//! and asserted on the parse tree would pass while the language was broken, and
//! would need rewriting every time the grammar moved.

use nh_runtime::{Ctx, SourceMap};
use pebble::{generated, Interp};

/// Runs a program, returning what it showed — or the diagnostics, rendered.
fn run(src: &str) -> Result<Vec<String>, String> {
    let mut sources = SourceMap::new();
    let file = sources.add("test.pebble", src.to_string());
    let mut cx = Ctx::new(sources);
    let mut interp = Interp::default();

    match generated::eval_source(&mut interp, &mut cx, file) {
        Ok(_) => Ok(interp.output.clone()),
        Err(diags) => Err(diags
            .iter()
            .map(|d| d.render(cx.sources()))
            .collect::<Vec<_>>()
            .join("\n")),
    }
}

fn shows(src: &str) -> Vec<String> {
    run(src).unwrap_or_else(|e| panic!("expected this to run:\n{e}"))
}

fn fails(src: &str) -> String {
    run(src).err().unwrap_or_else(|| panic!("expected this to fail: {src}"))
}

// ---- the operator table is real, not decorative -------------------------

#[test]
fn precedence_is_not_left_to_right() {
    assert_eq!(shows("show 4 * 7 + 2;"), ["30"]);
    assert_eq!(shows("show 2 + 4 * 7;"), ["30"]);
    assert_eq!(shows("show (2 + 4) * 7;"), ["42"]);
}

#[test]
fn short_circuit_does_not_evaluate_the_right_operand() {
    // `1 / 0` is an error, so if `&&` evaluated it eagerly this would fail.
    assert_eq!(shows("show 0 && (1 / 0);"), ["0"]);
}

// ---- values ---------------------------------------------------------------

#[test]
fn plus_concatenates_when_either_side_is_text() {
    assert_eq!(shows(r#"show "a" + 1;"#), ["a1"]);
    assert_eq!(shows(r#"show 1 + "a";"#), ["1a"]);
    assert_eq!(shows("show 1 + 1;"), ["2"]);
}

#[test]
fn points_add_componentwise_and_compare_by_value() {
    assert_eq!(shows("show (3, 4) + (1, 2);"), ["(4, 6)"]);
    assert_eq!(shows("show (3, 4) == (3, 4);"), ["true"]);
    assert_eq!(shows("show (3, 4) == (1, 2);"), ["false"]);
}

/// The origin is a point, so it is true. Nothing forced this; it is a decision
/// and therefore worth a test.
#[test]
fn the_origin_is_truthy() {
    assert_eq!(shows(r#"if (0, 0) { show "yes"; }"#), ["yes"]);
    assert_eq!(shows(r#"if 0 { show "no"; } else { show "zero is false"; }"#), ["zero is false"]);
}

// ---- control flow ---------------------------------------------------------

#[test]
fn a_while_loop_retests_its_condition() {
    assert_eq!(
        shows("let n = 0; while n < 3 { n = n + 1; } show n;"),
        ["3"]
    );
}

// ---- functions ------------------------------------------------------------

#[test]
fn a_function_can_recurse() {
    assert_eq!(
        shows("fn fact(n) { if n < 2 { return 1; } return n * fact(n - 1); } show fact(5);"),
        ["120"]
    );
}

/// The bug this guards is specific: a parameter kept in one shared map instead
/// of a per-call frame reads correctly on the way down and wrongly on the way
/// back up.
#[test]
fn recursion_does_not_clobber_the_callers_variables() {
    assert_eq!(
        shows(
            "fn fact(n) { if n < 2 { return 1; } return n * fact(n - 1); }\
             let n = 999; show fact(4); show n;"
        ),
        ["24", "999"]
    );
}

#[test]
fn falling_off_the_end_of_a_function_yields_null() {
    assert_eq!(shows("fn f(x) { let y = x; } show f(1);"), ["null"]);
}

#[test]
fn runaway_recursion_is_a_diagnostic_not_a_crash() {
    let e = fails("fn deep(k) { return deep(k + 1); } show deep(0);");
    assert!(e.contains("missing a base case"), "{e}");
}

#[test]
fn arity_is_checked() {
    let e = fails("fn f(a, b) { return a; } show f(1);");
    assert!(e.contains("takes 2 argument(s), got 1"), "{e}");
}

/// Pebble runs definitions in order rather than hoisting them. That is a
/// language decision, so it gets a test rather than a comment.
#[test]
fn a_function_must_be_defined_before_it_is_called() {
    let e = fails("show later(3); fn later(x) { return x; }");
    assert!(e.contains("`later` is not a function"), "{e}");
}

// ---- blocks ---------------------------------------------------------------

#[test]
fn a_frame_captures_only_its_own_output() {
    assert_eq!(
        shows(r#"show "before"; begin frame show "in"; end frame show "after";"#),
        ["before", "+----+", "| in |", "+----+", "after"]
    );
}

#[test]
fn frames_nest() {
    assert_eq!(
        shows(r#"begin frame show "a"; begin frame show "b"; end frame end frame"#),
        ["+-------+", "| a     |", "| +---+ |", "| | b | |", "| +---+ |", "+-------+"]
    );
}

// ---- errors ---------------------------------------------------------------

/// Recovery is the point: a broken statement must not cost you the good ones.
#[test]
fn a_broken_statement_does_not_stop_the_others() {
    let e = fails("show 1 + 1;\nshow @@@ ;\nshow 2 + 2;");
    assert!(e.contains("could not parse this `stmt`"), "{e}");
}

#[test]
fn dividing_by_zero_is_reported_rather_than_returning_infinity() {
    let e = fails("show 1 / 0;");
    assert!(e.contains("division by zero"), "{e}");
}

#[test]
fn an_undefined_name_says_which_one() {
    let e = fails("show nope;");
    assert!(e.contains("`nope` is not defined"), "{e}");
}
