//! End-to-end tests for the mini BASIC.
//!
//! The ones that matter are in the `lazy` section: they prove by **observation**
//! that a `FOR` body is not evaluated until the loop forces it, and is forced
//! exactly as many times as the loop says.

use basic_interp::{generated, BasicParser, Interp, Rule, Value};
use nh_runtime::{Ctx, SourceMap, Span};
use pest::Parser;

fn run(source: &str) -> Result<Interp, String> {
    let mut sources = SourceMap::new();
    let file = sources.add("t.bas", source);
    let text = sources.text(file).to_string();

    let mut pairs = BasicParser::parse(Rule::program, &text).map_err(|e| e.to_string())?;
    let program = pairs.next().expect("one program pair");

    let mut cx = Ctx::new(sources);
    cx.enter(Span::new(file, 0, 0));
    let mut interp = Interp::default();

    let tree = generated::ast::build_program(program, file).map_err(|e| cx.render(&e))?;
    generated::dispatch::eval_program(&mut interp, &tree, &mut cx)
        .map(|_| interp)
        .map_err(|e| cx.render(&e))
}

fn out(source: &str) -> Vec<String> {
    run(source)
        .unwrap_or_else(|e| panic!("{source}\n{e}"))
        .output
}

// ---------------------------------------------------------------------------
// PRINT
// ---------------------------------------------------------------------------

#[test]
fn print_writes_one_line_per_statement() {
    assert_eq!(out("PRINT 1\nPRINT 2\n"), vec!["1", "2"]);
}

#[test]
fn print_joins_its_arguments_with_tabs() {
    assert_eq!(out("PRINT 1, \"two\", 3\n"), vec!["1\ttwo\t3"]);
}

#[test]
fn a_bare_print_writes_a_blank_line() {
    assert_eq!(out("PRINT\nPRINT 1\n"), vec!["", "1"]);
}

#[test]
fn a_program_need_not_end_with_a_newline() {
    assert_eq!(out("PRINT 1"), vec!["1"]);
}

#[test]
fn comments_and_blank_lines_are_ignored() {
    assert_eq!(
        out("REM a note\n\nPRINT 1\n\nREM another\nPRINT 2\n"),
        vec!["1", "2"]
    );
}

// ---------------------------------------------------------------------------
// `lazy`: the loop body is not evaluated until it is forced
// ---------------------------------------------------------------------------

/// The proof. A loop whose range is empty must run its body **zero** times —
/// impossible if the body arrived evaluated, because evaluating it is what
/// produces the output.
#[test]
fn an_empty_range_never_runs_the_body() {
    assert!(
        out("FOR i = 10 TO 1\nPRINT \"never\"\nNEXT i\n").is_empty(),
        "the body ran despite an empty range"
    );
}

/// ...and a non-empty range runs it once per iteration, not once in total.
#[test]
fn the_body_runs_once_per_iteration() {
    assert_eq!(out("FOR i = 1 TO 3\nPRINT i\nNEXT i\n"), vec!["1", "2", "3"]);
}

#[test]
fn loops_nest() {
    assert_eq!(
        out("FOR a = 1 TO 2\nFOR b = 1 TO 2\nPRINT a, b\nNEXT b\nNEXT a\n"),
        vec!["1\t1", "1\t2", "2\t1", "2\t2"]
    );
}

#[test]
fn step_counts_by_something_other_than_one() {
    assert_eq!(out("FOR i = 1 TO 9 STEP 3\nPRINT i\nNEXT i\n"), vec!["1", "4", "7"]);
}

#[test]
fn a_negative_step_counts_down() {
    assert_eq!(out("FOR i = 5 TO 1 STEP -2\nPRINT i\nNEXT i\n"), vec!["5", "3", "1"]);
}

/// BASIC leaves the counter one step past the limit, and that is observable.
#[test]
fn the_counter_survives_the_loop() {
    let interp = run("FOR i = 1 TO 3\nNEXT i\nPRINT i\n").unwrap();
    assert_eq!(interp.output, vec!["4"]);
}

#[test]
fn the_body_can_write_to_variables_that_outlive_it() {
    assert_eq!(
        out("total = 0\nFOR i = 1 TO 10\ntotal = total + i\nNEXT i\nPRINT total\n"),
        vec!["55"]
    );
}

/// `NEXT` may name its loop or not. Naming a *different* one is an error, not
/// a silently mismatched block.
#[test]
fn next_may_omit_its_variable() {
    assert_eq!(out("FOR i = 1 TO 2\nPRINT i\nNEXT\n"), vec!["1", "2"]);
}

#[test]
fn next_naming_the_wrong_loop_is_reported() {
    let err = run("FOR i = 1 TO 2\nNEXT j\n").unwrap_err();
    assert!(err.contains("`NEXT j` closes a loop over `i`"), "{err}");
}

/// A loop that could never terminate is refused rather than hanging the test
/// suite, which is the only way to find out otherwise.
#[test]
fn a_zero_step_is_refused() {
    let err = run("FOR i = 1 TO 2 STEP 0\nNEXT i\n").unwrap_err();
    assert!(err.contains("would loop forever"), "{err}");
}

/// `WHILE` defers its **condition** as well as its body, which `FOR` does not
/// need to. Re-forcing the same `Deferred` is what re-evaluates the test.
#[test]
fn a_while_loop_retests_its_condition() {
    assert_eq!(
        out("i = 1\nWHILE i <= 3\nPRINT i\ni = i + 1\nWEND\n"),
        vec!["1", "2", "3"]
    );
}

/// If the condition were evaluated once, before the handler ran, this would
/// either loop forever or run the body once. It does neither.
#[test]
fn a_while_false_on_entry_never_runs_its_body() {
    assert!(out("WHILE 0\nPRINT \"never\"\nWEND\n").is_empty());
}

#[test]
fn while_loops_nest_inside_for_loops() {
    assert_eq!(
        out("FOR n = 1 TO 2\nk = 0\nWHILE k < n\nPRINT n, k\nk = k + 1\nWEND\nNEXT n\n"),
        vec!["1\t0", "2\t0", "2\t1"]
    );
}

// ---------------------------------------------------------------------------
// Case folding
// ---------------------------------------------------------------------------

/// `IDENT` folds case, so every binding to it is an `Ident` rather than a
/// `&str` — there is no unfolded string to look up by mistake.
#[test]
fn variables_ignore_case() {
    assert_eq!(out("Total = 7\nPRINT TOTAL\n"), vec!["7"]);
}

#[test]
fn keywords_ignore_case() {
    assert_eq!(out("for I = 1 to 2\nprint i\nnext i\n"), vec!["1", "2"]);
}

/// ...but a diagnostic reports the spelling the programmer used. Reporting the
/// folded form reads as a compiler bug.
#[test]
fn an_undefined_variable_is_reported_as_written() {
    let err = run("PRINT Missing\n").unwrap_err();
    assert!(err.contains("undefined variable `Missing`"), "{err}");
}

// ---------------------------------------------------------------------------
// Operators, all of which come from the table
// ---------------------------------------------------------------------------

#[test]
fn arithmetic_follows_the_declared_precedence() {
    assert_eq!(out("PRINT 2 + 3 * 4\n"), vec!["14"]);
    assert_eq!(out("PRINT (2 + 3) * 4\n"), vec!["20"]);
}

/// `MOD` sits between `+ -` and `* /`: tighter than addition, looser than
/// multiplication. Both directions are checked, because a tier that is only
/// tested against one neighbour can be in the wrong place and still pass.
#[test]
fn mod_sits_between_addition_and_multiplication() {
    // Tighter than `+`: `1 + (7 MOD 4)`, not `(1 + 7) MOD 4` (which is 0).
    assert_eq!(out("PRINT 1 + 7 MOD 4\n"), vec!["4"]);
    // Looser than `*`: `(2 * 3) MOD 4`, not `2 * (3 MOD 4)` (which is 6).
    assert_eq!(out("PRINT 2 * 3 MOD 4\n"), vec!["2"]);
}

/// BASIC's truth values, and the word operators that produce them.
#[test]
fn comparison_yields_minus_one_for_true() {
    assert_eq!(out("PRINT 3 < 4\n"), vec!["-1"]);
    assert_eq!(out("PRINT 3 > 4\n"), vec!["0"]);
    assert_eq!(out("PRINT 1 = 1 AND NOT 0\n"), vec!["-1"]);
    assert_eq!(out("PRINT 1 <> 1 OR 2 = 2\n"), vec!["-1"]);
}

#[test]
fn strings_compare_and_concatenate() {
    assert_eq!(out("PRINT \"a\" + \"b\"\n"), vec!["ab"]);
    assert_eq!(out("PRINT \"a\" = \"a\"\n"), vec!["-1"]);
}

#[test]
fn dividing_by_zero_is_an_error_not_an_infinity() {
    let err = run("PRINT 1 / 0\n").unwrap_err();
    assert!(err.contains("division by zero"), "{err}");
}

/// A runtime error carries the location of the node that raised it, with no
/// handler threading spans by hand.
#[test]
fn errors_carry_a_location() {
    let err = run("PRINT 1\nPRINT nope\n").unwrap_err();
    assert!(err.contains("t.bas:2:"), "expected a line-2 location:\n{err}");
}

// ---------------------------------------------------------------------------
// The shipped sample
// ---------------------------------------------------------------------------

#[test]
fn the_shipped_sample_runs() {
    let interp = run(include_str!("../sample.bas")).unwrap();
    assert_eq!(interp.output.first().unwrap(), "times table");
    assert!(
        interp.output.iter().all(|l| l != "never printed"),
        "the empty loop ran its body"
    );
    assert_eq!(interp.output.last().unwrap(), "3 < 4 AND NOT 0 =\t-1");
    assert!(
        interp.output.contains(&"---------------".to_string()),
        "the subroutine ran: {:?}",
        interp.output
    );
    // The prime sieve exercises `EXIT WHILE` and `CONTINUE FOR` together, and
    // getting either wrong changes which numbers come out.
    for p in ["2", "3", "5", "7", "11", "13", "17", "19"] {
        assert!(interp.output.contains(&p.to_string()), "missing prime {p}");
    }
    for c in ["4", "9", "15"] {
        assert!(!interp.output.contains(&c.to_string()), "{c} is not prime");
    }
    assert!(interp.output.contains(&"6! =\t720".to_string()), "{:?}", interp.output);
    assert_eq!(interp.vars.get("total"), Some(&Value::Num(55.0)));
}

// ---------------------------------------------------------------------------
// Subroutines: a piece of program held as a value
// ---------------------------------------------------------------------------

/// The M7 payoff. `SUB` **keeps** its body and `CALL` runs it later — which
/// means a handler stored unevaluated program on the interpreter and ran it
/// after returning. Under the borrowed `Deferred` of M2–M6 this was not
/// expressible at all (DESIGN.md §9).
#[test]
fn a_subroutine_body_outlives_the_handler_that_defined_it() {
    assert_eq!(
        out("SUB hi\nPRINT \"hi\"\nEND SUB\nPRINT \"before\"\nCALL hi\n"),
        vec!["before", "hi"]
    );
}

/// Defined once, run many times — the body is shared, not copied.
#[test]
fn a_subroutine_runs_once_per_call() {
    assert_eq!(
        out("SUB hi\nPRINT \"hi\"\nEND SUB\nCALL hi\nCALL hi\nCALL hi\n"),
        vec!["hi", "hi", "hi"]
    );
}

#[test]
fn a_subroutine_may_call_another() {
    assert_eq!(
        out("SUB a\nPRINT 1\nEND SUB\nSUB b\nCALL a\nPRINT 2\nEND SUB\nCALL b\n"),
        vec!["1", "2"]
    );
}

#[test]
fn a_subroutine_sees_and_updates_globals() {
    assert_eq!(
        out("n = 0\nSUB bump\nn = n + 1\nEND SUB\nFOR i = 1 TO 3\nCALL bump\nNEXT i\nPRINT n\n"),
        vec!["3"]
    );
}

/// Names fold, like every other identifier in this language.
#[test]
fn subroutine_names_ignore_case() {
    assert_eq!(out("SUB Greet\nPRINT 1\nEND SUB\ncall GREET\n"), vec!["1"]);
}

#[test]
fn calling_something_undefined_is_reported_as_written() {
    let err = run("CALL Missing\n").unwrap_err();
    assert!(err.contains("undefined subroutine `Missing`"), "{err}");
}

#[test]
fn defining_the_same_subroutine_twice_is_refused() {
    let err = run("SUB a\nPRINT 1\nEND SUB\nSUB a\nPRINT 2\nEND SUB\n").unwrap_err();
    assert!(err.contains("`SUB a` is already defined"), "{err}");
}

/// Recursion is possible now, so runaway recursion is too. It has to report
/// rather than overflow the stack, because a stack overflow aborts the process
/// with no diagnostic and no location.
#[test]
fn infinite_recursion_reports_instead_of_overflowing() {
    let err = run("SUB loopy\nCALL loopy\nEND SUB\nCALL loopy\n").unwrap_err();
    assert!(err.contains("infinite recursion"), "{err}");
}

// ---------------------------------------------------------------------------
// `GOTO`: a jump, not a fold
// ---------------------------------------------------------------------------

/// The construct this whole design was blocked on. It needs two things at once,
/// and the owned AST supplies both: the driver **inspects** each line's number
/// without running it, and the lines **outlive** any single evaluation so it can
/// go back to one it already passed (DESIGN.md §9).
#[test]
fn a_backward_goto_loops() {
    assert_eq!(
        out("n = 3\n10 PRINT n\nn = n - 1\nIF n > 0 THEN GOTO 10\nPRINT \"done\"\n"),
        vec!["3", "2", "1", "done"]
    );
}

#[test]
fn a_forward_goto_skips_what_it_jumps_over() {
    assert_eq!(
        out("GOTO 20\nPRINT \"skipped\"\n20 PRINT \"landed\"\n"),
        vec!["landed"]
    );
}

/// A jump out of a loop body unwinds through the loop handler, because a signal
/// propagates exactly like an error until something catches it.
#[test]
fn a_goto_escapes_an_enclosing_loop() {
    assert_eq!(
        out("FOR i = 1 TO 9\nPRINT i\nIF i = 2 THEN GOTO 50\nNEXT i\n50 PRINT \"out\"\n"),
        vec!["1", "2", "out"]
    );
}

#[test]
fn jumping_to_a_line_that_does_not_exist_is_reported() {
    let err = run("GOTO 99\n").unwrap_err();
    assert!(err.contains("there is no line numbered 99"), "{err}");
}

/// Two lines with one number makes every jump to it ambiguous, and quietly
/// taking the first is very hard to notice from inside the program.
#[test]
fn a_duplicate_line_number_is_refused() {
    let err = run("10 PRINT 1\n10 PRINT 2\n").unwrap_err();
    assert!(err.contains("used more than once"), "{err}");
}

/// `IF` guards its body with `lazy`, which is what lets a backward `GOTO`
/// terminate at all.
#[test]
fn a_false_condition_runs_nothing() {
    assert!(out("IF 0 THEN PRINT \"never\"\n").is_empty());
}

#[test]
fn a_conditional_uses_the_languages_own_truthiness() {
    // `0` is falsy for `AND`, so it must be falsy for `IF` too.
    assert!(out("IF 0 THEN PRINT 1\n").is_empty());
    assert_eq!(out("IF 5 THEN PRINT 1\n"), vec!["1"]);
}

// ---------------------------------------------------------------------------
// `EXIT` and `CONTINUE`: signals, and which construct catches them
// ---------------------------------------------------------------------------

#[test]
fn exit_for_leaves_the_loop() {
    assert_eq!(
        out("FOR i = 1 TO 9\nIF i > 3 THEN EXIT FOR\nPRINT i\nNEXT i\n"),
        vec!["1", "2", "3"]
    );
}

/// A loop cut short leaves the counter where it stopped, not past the limit.
#[test]
fn exit_for_leaves_the_counter_where_it_stopped() {
    assert_eq!(
        out("FOR i = 1 TO 9\nIF i > 3 THEN EXIT FOR\nNEXT i\nPRINT i\n"),
        vec!["4"]
    );
}

/// `CONTINUE` skips the rest of the body but still advances the counter — the
/// difference from `EXIT` is one `break` target.
#[test]
fn continue_for_skips_the_rest_of_the_body() {
    assert_eq!(
        out("FOR j = 1 TO 5\nIF j MOD 2 = 0 THEN CONTINUE FOR\nPRINT j\nNEXT j\n"),
        vec!["1", "3", "5"]
    );
}

#[test]
fn exit_and_continue_work_in_a_while() {
    assert_eq!(
        out("k = 0\nWHILE 1\nk = k + 1\nIF k > 3 THEN EXIT WHILE\nPRINT k\nWEND\n"),
        vec!["1", "2", "3"]
    );
    assert_eq!(
        out("k = 0\nWHILE k < 5\nk = k + 1\nIF k = 3 THEN CONTINUE WHILE\nPRINT k\nWEND\n"),
        vec!["1", "2", "4", "5"]
    );
}

/// **The reason labels are strings.** An `EXIT FOR` raised inside a nested
/// `WHILE` is not the `WHILE`'s signal, so it passes straight through to the
/// loop that owns it. Nesting resolves itself with no depth counting.
#[test]
fn a_signal_passes_through_a_loop_it_does_not_name() {
    assert_eq!(
        out("FOR a = 1 TO 5\nk = 0\nWHILE k < 5\nk = k + 1\nPRINT a, k\nIF a = 2 THEN EXIT FOR\nWEND\nNEXT a\nPRINT \"after\"\n"),
        vec!["1\t1", "1\t2", "1\t3", "1\t4", "1\t5", "2\t1", "after"]
    );
}

#[test]
fn exit_sub_returns_early() {
    assert_eq!(
        out("SUB early\nPRINT \"in\"\nEXIT SUB\nPRINT \"never\"\nEND SUB\nCALL early\nPRINT \"back\"\n"),
        vec!["in", "back"]
    );
}

/// A subroutine is a **boundary**. Without this, `EXIT FOR` inside a sub would
/// unwind into whatever loop happened to call it — dynamically enclosing but
/// not lexically, so the jump would land somewhere the source does not show.
#[test]
fn loop_control_cannot_cross_out_of_a_subroutine() {
    let err = run("SUB bad\nEXIT FOR\nEND SUB\nFOR i = 1 TO 3\nCALL bad\nNEXT i\n").unwrap_err();
    assert!(err.contains("cannot cross out of a `SUB`"), "{err}");
}

/// An uncaught signal is a real error, and it reports against the label — which
/// is why the label is spelled the way the language spells it.
#[test]
fn an_uncaught_exit_reports_in_the_languages_own_words() {
    let err = run("EXIT SUB\n").unwrap_err();
    assert!(
        err.contains("`EXIT SUB` is not inside anything that handles it"),
        "{err}"
    );
    assert!(err.contains("t.bas:1:"), "and says where:\n{err}");
}

#[test]
fn exit_for_outside_a_loop_is_reported_too() {
    let err = run("EXIT FOR\n").unwrap_err();
    assert!(err.contains("`EXIT FOR` is not inside"), "{err}");
}

// ---------------------------------------------------------------------------
// Functions: arguments, a return value, and a call inside an expression
// ---------------------------------------------------------------------------
//
// `SUB` is the easy half — no arguments, no result, statement position only.
// These cover the half that actually stresses the design.

#[test]
fn a_function_returns_a_value() {
    assert_eq!(out("FUNCTION d(n)\nRETURN n * 2\nEND FUNCTION\nPRINT d(21)\n"), vec!["42"]);
}

#[test]
fn a_function_takes_several_arguments_in_order() {
    assert_eq!(
        out("FUNCTION sub2(a, b)\nRETURN a - b\nEND FUNCTION\nPRINT sub2(10, 3)\n"),
        vec!["7"],
        "arguments must bind positionally, not by name"
    );
}

#[test]
fn a_function_may_take_none() {
    assert_eq!(out("FUNCTION four()\nRETURN 4\nEND FUNCTION\nPRINT four()\n"), vec!["4"]);
}

/// A call is an ordinary operand, so the operator driver folds it as an atom
/// and precedence applies around it unchanged.
#[test]
fn a_call_is_an_operand_like_any_other() {
    assert_eq!(
        out("FUNCTION d(n)\nRETURN n * 2\nEND FUNCTION\nPRINT d(3) + d(1) * 2\n"),
        vec!["10"],
        "`d(3) + (d(1) * 2)`, not `(d(3) + d(1)) * 2`"
    );
    assert_eq!(
        out("FUNCTION d(n)\nRETURN n * 2\nEND FUNCTION\nPRINT d(d(3))\n"),
        vec!["12"],
        "an argument may itself be a call"
    );
}

/// **Where per-call frames earn their keep.** Every frame binds its own `n`; a
/// single shared map would have the innermost call overwrite its callers.
#[test]
fn recursion_gives_each_call_its_own_parameters() {
    assert_eq!(
        out("FUNCTION fact(n)\nIF n <= 1 THEN RETURN 1\nRETURN n * fact(n - 1)\nEND FUNCTION\nPRINT fact(6)\n"),
        vec!["720"]
    );
}

/// ...and a parameter is local, so it does not clobber a global of the same
/// name. This is the one thing `SUB` never had to get right.
#[test]
fn a_parameter_does_not_leak_into_the_caller() {
    assert_eq!(
        out("FUNCTION f(n)\nn = n + 1\nRETURN n\nEND FUNCTION\nn = 99\nPRINT f(1), n\n"),
        vec!["2\t99"]
    );
}

/// A non-parameter assignment inside a function is still global, which is what
/// a BASIC programmer expects.
#[test]
fn a_function_can_still_reach_globals() {
    assert_eq!(
        out("total = 0\nFUNCTION add(n)\ntotal = total + n\nRETURN total\nEND FUNCTION\nPRINT add(2)\nPRINT add(3)\n"),
        vec!["2", "5"]
    );
}

/// `RETURN` is not a loop signal, so a `FOR` in the way ignores it and it
/// unwinds to the call — the same pass-through that makes `EXIT FOR` work.
#[test]
fn return_escapes_an_enclosing_loop() {
    assert_eq!(
        out("FUNCTION first()\nFOR i = 7 TO 9\nRETURN i\nNEXT i\nEND FUNCTION\nPRINT first()\n"),
        vec!["7"]
    );
}

#[test]
fn calling_an_undefined_function_is_reported_as_written() {
    let err = run("PRINT Nope(1)\n").unwrap_err();
    assert!(err.contains("undefined function `Nope`"), "{err}");
}

#[test]
fn the_wrong_number_of_arguments_is_refused() {
    let err = run("FUNCTION f(a)\nRETURN a\nEND FUNCTION\nPRINT f(1, 2)\n").unwrap_err();
    assert!(err.contains("takes 1 argument(s), got 2"), "{err}");
}

/// Falling off the end has no value to give back, and inventing one would hide
/// the mistake.
#[test]
fn a_function_that_never_returns_is_an_error() {
    let err = run("FUNCTION f(a)\nPRINT a\nEND FUNCTION\nPRINT f(1)\n").unwrap_err();
    assert!(err.contains("ended without a `RETURN`"), "{err}");
}

/// Two parameters with one name would make the second silently win.
#[test]
fn a_duplicate_parameter_name_is_refused() {
    let err = run("FUNCTION f(a, a)\nRETURN a\nEND FUNCTION\n").unwrap_err();
    assert!(err.contains("`a` is bound twice"), "{err}");
}

#[test]
fn defining_the_same_function_twice_is_refused() {
    let err = run("FUNCTION f()\nRETURN 1\nEND FUNCTION\nFUNCTION f()\nRETURN 2\nEND FUNCTION\n")
        .unwrap_err();
    assert!(err.contains("`FUNCTION f` is already defined"), "{err}");
}

/// A function is a boundary, exactly as a `SUB` is.
#[test]
fn loop_control_cannot_cross_out_of_a_function() {
    let err = run("FUNCTION f()\nEXIT FOR\nEND FUNCTION\nFOR i = 1 TO 3\nPRINT f()\nNEXT i\n")
        .unwrap_err();
    assert!(err.contains("cannot cross out of a `FUNCTION`"), "{err}");
}

#[test]
fn function_names_ignore_case() {
    assert_eq!(out("FUNCTION Twice(n)\nRETURN n * 2\nEND FUNCTION\nPRINT TWICE(4)\n"), vec!["8"]);
}

/// Runaway recursion has to report rather than abort the process.
#[test]
fn runaway_function_recursion_reports() {
    let err = run("FUNCTION f(n)\nRETURN f(n)\nEND FUNCTION\nPRINT f(1)\n").unwrap_err();
    assert!(err.contains("infinite recursion"), "{err}");
}
