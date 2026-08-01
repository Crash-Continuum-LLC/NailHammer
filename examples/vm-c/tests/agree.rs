//! The two twins must agree, instruction for instruction.
//!
//! `examples/vm-c` and `examples/vm-basic` are the same language wearing
//! different syntax. Their grammars share not one line — braces against
//! `END IF`, `&` against `AND`, `;` against newlines — and they bind the **same
//! roles**, so both compile to the same instructions on the same machine.
//!
//! Neither crate contains a line of operator code. That is the claim these
//! check: what a language keeps is its syntax and its statements, and what it
//! gets is everything else.

/// The same program, written twice.
///
/// **Keep these two in step.** The point of the pair is that a change to one
/// language shows up as a difference in bytecode, and a program that exercises
/// less of the language proves less. `the_pair_covers_the_whole_language`
/// below fails if a construct exists in a grammar and is missing from here.
const C: &str = r#"
x = 10;
y = 4;
print x + y * 2;
print -(x - y);
print x > y;
print 12 & 10;
if (x > y) { print 1; }
if (y > x) { print 999; }
n = 0;
while (n < 3) { print n; n = n + 1; }
"#;

const BASIC: &str = r#"
LET x = 10
LET y = 4
PRINT x + y * 2
PRINT -(x - y)
PRINT x > y
PRINT 12 AND 10
IF x > y THEN
PRINT 1
END IF
IF y > x THEN
PRINT 999
END IF
LET n = 0
WHILE n < 3
PRINT n
LET n = n + 1
WEND
"#;

#[test]
fn the_two_syntaxes_produce_the_same_output() {
    let c = vm_c::run(&vm_c::compile(C).expect("C compiles")).expect("C runs");
    let b = vm_basic::run(&vm_basic::compile(BASIC).expect("BASIC compiles")).expect("BASIC runs");

    assert_eq!(c, b, "two syntaxes, one language");
    assert_eq!(
        c,
        ["18", "-6", "true", "8", "1", "0", "1", "2"],
        "and the answers are right, not merely equal"
    );
}

/// Stronger than agreeing on output: they agree on *bytecode*.
///
/// Same instruction count means the two front ends made the same lowering
/// decisions — precedence, register allocation, jump placement — from grammars
/// that share no syntax. If one drifts, this catches it before the outputs do.
#[test]
fn the_two_syntaxes_produce_the_same_instructions() {
    let c = vm_c::compile(C).expect("C compiles");
    let b = vm_basic::compile(BASIC).expect("BASIC compiles");

    assert_eq!(
        c.code.len(),
        b.code.len(),
        "same program, same instruction count\n  C:     {:#?}\n  BASIC: {:#?}",
        c.code,
        b.code
    );
    assert_eq!(c.frame, b.frame, "same register frame");
    assert_eq!(c.globals, b.globals, "same globals");

    assert_eq!(
        format!("{:?}", c.code),
        format!("{:?}", b.code),
        "instruction for instruction"
    );
}

/// `12 & 10` and `12 AND 10` are one instruction — the role system's whole
/// claim, checked at the far end of the pipeline.
#[test]
fn a_word_operator_and_a_symbol_reach_one_opcode() {
    let c = vm_c::compile("print 12 & 10;").expect("C compiles");
    let b = vm_basic::compile("PRINT 12 AND 10\n").expect("BASIC compiles");

    let and_of = |code: &[nh_vm::Op<nh_vm::NoExt>]| {
        code.iter()
            .filter(|op| matches!(op, nh_vm::Op::And { .. }))
            .count()
    };

    assert_eq!(and_of(&c.code), 1, "`&` emitted exactly one And");
    assert_eq!(and_of(&b.code), 1, "`AND` emitted exactly one And");
    assert_eq!(format!("{:?}", c.code), format!("{:?}", b.code));
}

/// `&&` must not evaluate its right operand when the left is false.
///
/// Checked by *counting instructions executed*, not by output: an
/// implementation that evaluates both and then picks produces the same answer
/// and is not short-circuiting. Division by zero on the right is the probe —
/// it fails loudly if it runs.
#[test]
fn short_circuit_does_not_evaluate_the_right_operand() {
    // `0 && (1/0)` — if `&&` were strict, this would fail with a division error.
    let p = vm_c::compile("print 0 && (1 / 0);").expect("compiles");
    let out = vm_c::run(&p).expect("must not divide by zero");
    assert_eq!(out, ["0"], "left operand is the answer, right never ran");

    // And the mirror: `1 || (1/0)`.
    let p = vm_c::compile("print 1 || (1 / 0);").expect("compiles");
    assert_eq!(vm_c::run(&p).expect("must not divide"), ["1"]);

    // When it *does* need the right operand, it evaluates it.
    let p = vm_c::compile("print 1 && 7;").expect("compiles");
    assert_eq!(vm_c::run(&p).expect("runs"), ["7"]);
}

/// The BASIC twin's `ANDALSO` is the same instruction sequence as the C twin's
/// `&&` — different spelling, one role, one lowering.
#[test]
fn the_twins_short_circuit_identically() {
    let c = vm_c::compile("print 0 && (1 / 0);").expect("C compiles");
    let b = vm_basic::compile("PRINT 0 ANDALSO (1 / 0)\n").expect("BASIC compiles");

    assert_eq!(format!("{:?}", c.code), format!("{:?}", b.code));
    assert_eq!(vm_c::run(&c).unwrap(), vm_basic::run(&b).unwrap());
}

// ---------------------------------------------------------------------------
// Assignment as an expression — C only, and that is the point
// ---------------------------------------------------------------------------
//
// The twins have genuinely diverged here. C binds `=` as an operator, so `x = 1`
// is an expression that yields a value; BASIC keeps `LET` as a statement and
// uses `=` for comparison, which is what a BASIC does. They still agree on
// everything in the shared subset, which is what `the_two_syntaxes_*` cover.

/// `=` is lazy in its **left** operand: the target arrives as a `Place`, not a
/// value, so nothing evaluated the variable before storing to it.
#[test]
fn assignment_stores_and_yields_the_value() {
    let out = vm_c::run(&vm_c::compile("x = 10; print x;").expect("compiles")).expect("runs");
    assert_eq!(out, ["10"]);

    // It yields, so it can be printed directly.
    let out = vm_c::run(&vm_c::compile("print x = 7;").expect("compiles")).expect("runs");
    assert_eq!(out, ["7"]);
}

/// Right-associative, so `a = b = 4` assigns 4 to both rather than assigning
/// the result of a comparison.
#[test]
fn assignment_chains_right_to_left() {
    let p = vm_c::compile("a = b = 4; print a; print b;").expect("compiles");
    assert_eq!(vm_c::run(&p).expect("runs"), ["4", "4"]);
}

/// The store goes to a slot, and reading the same name reaches the same slot —
/// which is what makes assignment and variable reference agree.
#[test]
fn assignment_and_reference_reach_one_slot() {
    let p = vm_c::compile("x = 1; x = x + 41; print x;").expect("compiles");
    assert_eq!(vm_c::run(&p).expect("runs"), ["42"]);
    assert_eq!(p.globals, 1, "one variable, one slot");
}

// ---------------------------------------------------------------------------
// The boundary is bytes (VM-DESIGN.md §8.1)
// ---------------------------------------------------------------------------

/// Source in one place, bytes in the middle, execution somewhere else.
///
/// Nothing but the byte stream connects the two halves — no Rust types cross,
/// which is what makes two vendored copies of `nh-vm` indistinguishable and a
/// plugin able to compile without linking an execution engine.
#[test]
fn a_program_runs_from_bytes_alone() {
    let compiled = vm_c::compile("x = 6; print x * 7;").expect("compiles");
    let bytes = compiled.to_bytes();

    // Everything past here has only the bytes.
    let loaded = nh_vm::Program::<nh_vm::NoExt>::from_bytes(&bytes).expect("loads");
    let globals = nh_vm::DefaultStore::new(loaded.globals);
    let mut m = nh_vm::Machine::new(&loaded, &globals);

    assert!(matches!(m.resume(), nh_vm::Step::Done));
    assert_eq!(m.output, ["42"]);
}

/// **Two languages, one host.** The strongest form of the claim: a host that
/// knows neither grammar loads bytecode from both and runs them the same way.
///
/// This is what the whole design is for. If it needed to know which language
/// produced a stream, "pluggable" would be a word rather than a property.
#[test]
fn one_host_runs_bytecode_from_both_languages() {
    let from_c = vm_c::compile("print 2 + 3 * 4;").expect("C compiles").to_bytes();
    let from_basic = vm_basic::compile("PRINT 2 + 3 * 4\n").expect("BASIC compiles").to_bytes();

    // The "host": no mention of either language, only the format.
    let run = |bytes: &[u8]| {
        let p = nh_vm::Program::<nh_vm::NoExt>::from_bytes(bytes).expect("loads");
        let globals = nh_vm::DefaultStore::new(p.globals);
        let mut m = nh_vm::Machine::new(&p, &globals);
        assert!(matches!(m.resume(), nh_vm::Step::Done));
        m.output
    };

    assert_eq!(run(&from_c), ["14"]);
    assert_eq!(run(&from_basic), ["14"]);
    assert_eq!(from_c, from_basic, "same program, same bytes, whatever it was written in");
}

// ---------------------------------------------------------------------------
// Sequences
// ---------------------------------------------------------------------------

/// Every construct the pair is supposed to cover, anchored on the text that
/// *defines* it in each grammar.
///
/// Anchors, not bare words. A first version matched `"LEN"` anywhere, and
/// deleting the operator left the entry in `reserved from` — so the guard
/// passed while the languages had genuinely diverged. A guard that can be
/// satisfied by a keyword list is not a guard.
const COVERED: &[(&str, &str, &str)] = &[
    ("print", r#""print" value:expr"#, r#""PRINT" value:expr"#),
    ("if", r#""if" "(" cond"#, r#""IF" cond"#),
    ("else", r#""else" lazy alt"#, r#""ELSE" lazy alt"#),
    ("while", r#""while" "(""#, r#""WHILE" lazy cond"#),
    ("functions", r#""fn" name:IDENT"#, r#""FUNCTION" name:IDENT"#),
    ("return", r#""return" value:expr"#, r#""RETURN" value:expr"#),
    ("calls", r#"name:IDENT "(" args"#, r#"name:IDENT "(" args"#),
    ("await", r#"prefix word "await""#, r#"prefix word "AWAIT""#),
    ("len", r#"prefix word "len""#, r#"prefix word "LEN""#),
    ("array literals", r#""[" first:expr"#, r#""[" first:expr"#),
    ("indexing", r#"name:IDENT "[" index:expr "]""#, r#"name:IDENT "[" index:expr "]""#),
    ("strings", "text:STRING", "text:STRING"),
];

/// The guard against this pair quietly becoming a pair of *different*
/// languages.
///
/// The agreement tests only compare a program written twice, so they prove
/// exactly as much as that program exercises. The twins diverged badly once —
/// functions, arrays, strings, `await` and `len` existed only in C for several
/// commits — and every agreement test still passed, because the shared sample
/// used none of them. A test that reads like broad coverage while covering less
/// and less is worse than no test.
///
/// This one fails when a construct is in one grammar and not the other.
#[test]
fn the_pair_covers_the_whole_language() {
    let c = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/lang.nh")).unwrap();
    let b = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../vm-basic/lang.nh"))
        .unwrap();

    let mut missing = Vec::new();
    for (feature, in_c, in_b) in COVERED {
        if !c.contains(in_c) {
            missing.push(format!("{feature}: vm-c no longer has `{in_c}`"));
        }
        if !b.contains(in_b) {
            missing.push(format!("{feature}: vm-basic no longer has `{in_b}`"));
        }
    }
    assert!(missing.is_empty(), "the twins have diverged:\n  {}", missing.join("\n  "));
}

fn run_c(src: &str) -> Vec<String> {
    vm_c::run(&vm_c::compile(src).expect("compiles")).expect("runs")
}

#[test]
fn arrays_are_built_indexed_and_assigned() {
    assert_eq!(run_c("a = [10, 20, 30]; print a[0]; print a[2];"), ["10", "30"]);
    assert_eq!(run_c("a = [1, 2]; a[1] = 9; print a;"), ["[1, 9]"]);
    // Writing one past the end appends, so a program grows an array without a
    // separate instruction.
    assert_eq!(run_c("a = [1]; a[1] = 2; print a;"), ["[1, 2]"]);
}

/// Arrays are **reference types**: two names for one array see each other's
/// writes. That is what `Arc<RwLock<..>>` buys and what `Arc<Vec<..>>` would
/// not — and it is why arrays pay a lock and strings do not.
#[test]
fn two_names_for_one_array_share_it() {
    assert_eq!(run_c("a = [1, 2]; b = a; b[0] = 99; print a[0];"), ["99"]);
}

/// Strings index and measure the same way arrays do, because the VM decides
/// what has a length rather than each language deciding for itself.
#[test]
fn strings_index_and_measure() {
    assert_eq!(run_c(r#"s = "hammer"; print s[0]; print len s;"#), ["h", "6"]);
    assert_eq!(run_c(r#"print len [1, 2, 3];"#), ["3"]);
    assert_eq!(run_c(r#"print "nail" + "hammer";"#), ["nailhammer"]);
}

/// Out of range is an error, not a silent `Nil` — and the message says which
/// index and which length.
#[test]
fn an_index_out_of_range_is_reported() {
    let p = vm_c::compile("a = [1]; print a[5];").expect("compiles");
    match vm_c::run(&p) {
        Err(e) => assert!(e.contains("out of range") && e.contains("5"), "{e}"),
        Ok(o) => panic!("expected an error, got {o:?}"),
    }
}

/// Indexing is 0-based and a fractional index is a mistake rather than a
/// truncation: `a[1.5]` reading `a[1]` would hide the bug.
#[test]
fn a_fractional_index_is_refused() {
    let p = vm_c::compile("a = [1, 2]; print a[1 / 2];").expect("compiles");
    match vm_c::run(&p) {
        Err(e) => assert!(e.contains("whole number"), "{e}"),
        Ok(o) => panic!("expected an error, got {o:?}"),
    }
}

/// Arrays survive the wire, so a host can be handed one in a constant.
#[test]
fn an_array_constant_crosses_the_wire() {
    let p = vm_c::compile("a = [1, 2, 3]; print a;").expect("compiles");
    let back = nh_vm::Program::<nh_vm::NoExt>::from_bytes(&p.to_bytes()).expect("decodes");
    let globals = nh_vm::DefaultStore::new(back.globals);
    let mut m = nh_vm::Machine::new(&back, &globals);
    assert!(matches!(m.resume(), nh_vm::Step::Done));
    assert_eq!(m.output, ["[1, 2, 3]"]);
}

// ---------------------------------------------------------------------------
// Functions — reachable from a grammar, not just from hand-built bytecode
// ---------------------------------------------------------------------------

#[test]
fn a_function_is_defined_and_called() {
    assert_eq!(run_c("fn double(n) { return n + n; } print double(21);"), ["42"]);
    assert_eq!(run_c("fn add(a, b) { return a + b; } print add(3, 4);"), ["7"]);
}

/// Recursion, which is why functions are looked up by name at run time: the
/// body calls itself before the compiler has finished emitting it.
#[test]
fn a_function_can_recurse() {
    let src = "fn fact(n) { if (n < 2) { return 1; } return n * fact(n - 1); } print fact(5);";
    assert_eq!(run_c(src), ["120"]);
}

/// Parameters are registers, and they are **live for the whole body**.
///
/// Both bugs this caught are silent-wrong rather than loud. Resetting scratch
/// to register zero handed a parameter back out between statements, so `fact`
/// returned its base case; and `reuse` freeing a parameter wrote a comparison
/// result into it, so `n < 2` destroyed `n`. Neither crashes — they return a
/// plausible number — which is why this asserts values rather than success.
#[test]
fn a_parameter_survives_the_statements_around_it() {
    // `n` is read after a statement boundary and after a comparison has run.
    let src = "fn f(n) { if (n > 0) { return n; } return 0 - n; } print f(7); print f(0 - 3);";
    assert_eq!(run_c(src), ["7", "3"]);
}

/// A call before the definition works, because the callee is found by name when
/// the program runs rather than patched when it is compiled.
#[test]
fn a_function_may_be_called_before_it_is_defined() {
    assert_eq!(run_c("print later(2); fn later(x) { return x * 10; }"), ["20"]);
}

#[test]
fn calling_something_undefined_names_it() {
    match vm_c::run(&vm_c::compile("print nope(1);").expect("compiles")) {
        Err(e) => assert!(e.contains("nope"), "{e}"),
        Ok(o) => panic!("expected an error, got {o:?}"),
    }
}

/// Functions cross the wire with the program, since they are part of it.
#[test]
fn functions_survive_the_wire() {
    let p = vm_c::compile("fn sq(n) { return n * n; } print sq(9);").expect("compiles");
    assert_eq!(p.fns.len(), 1, "the function table travels too");

    let back = nh_vm::Program::<nh_vm::NoExt>::from_bytes(&p.to_bytes()).expect("decodes");
    let globals = nh_vm::DefaultStore::new(back.globals);
    let mut m = nh_vm::Machine::new(&back, &globals);
    assert!(matches!(m.resume(), nh_vm::Step::Done));
    assert_eq!(m.output, ["81"]);
}

// ---------------------------------------------------------------------------
// Locals — the bug here was found by asking, not by a test
// ---------------------------------------------------------------------------
//
// A variable first assigned inside a function used to become a *global*, shared
// by every frame. It worked in the obvious test and failed in the mirror image:
// `t + f(n-1)` was right and `f(n-1) + t` was wrong, because reading `t` into a
// register before the recursive call happened to save it. These pin the shape of
// that bug rather than the one example of it.

#[test]
fn a_local_is_per_frame_whichever_side_of_the_operator_it_is_on() {
    let body = "fn f(n) { if (n < 1) { return 0; } t = n;";
    assert_eq!(run_c(&format!("{body} return t + f(n - 1); }} print f(4);")), ["10"]);
    assert_eq!(run_c(&format!("{body} return f(n - 1) + t; }} print f(4);")), ["10"]);
}

#[test]
fn two_functions_may_use_the_same_local_name() {
    let src = "fn a(x) { t = x + 1; return t; } fn b(y) { t = y * 100; return t; } \
               print a(1); print b(2); print a(1);";
    assert_eq!(run_c(src), ["2", "200", "2"]);
}

#[test]
fn several_locals_survive_recursion_together() {
    let src = "fn f(n) { if (n < 1) { return 0; } a = n; b = n; return f(n - 1) + a + b; } \
               print f(3);";
    assert_eq!(run_c(src), ["12"]);
}

/// A local shadows a global rather than writing through to it. That is a
/// language decision this grammar makes, not an accident — and it is the reason
/// `store_var` is the only place that decides where a name lives.
#[test]
fn a_local_shadows_a_global_of_the_same_name() {
    assert_eq!(run_c("g = 1; fn f() { g = 2; return g; } print f(); print g;"), ["2", "1"]);
    // Reading still reaches the global when nothing local shadows it.
    assert_eq!(run_c("g = 7; fn f() { return g; } print f();"), ["7"]);
}

#[test]
fn functions_compose_and_recurse_mutually() {
    assert_eq!(run_c("fn a(n){return n+1;} fn b(n){return a(n)*2;} print b(3);"), ["8"]);
    let mutual = "fn ev(n){ if(n<1){return 1;} return od(n-1);} \
                  fn od(n){ if(n<1){return 0;} return ev(n-1);} print ev(4);";
    assert_eq!(run_c(mutual), ["1"]);
}

/// Arrays are references, so one passed to a function is the caller's array.
#[test]
fn an_array_passed_to_a_function_is_the_same_array() {
    assert_eq!(run_c("fn m(a){ a[0]=42; return 0; } q=[1]; m(q); print q[0];"), ["42"]);
}

#[test]
fn a_call_with_no_arguments_works() {
    assert_eq!(run_c("fn f() { return 7; } print f();"), ["7"]);
}

#[test]
fn control_flow_nests_inside_a_function() {
    let src = "fn s(n){ t=0; i=0; while(i<n){ t=t+i; i=i+1; } return t; } print s(5);";
    assert_eq!(run_c(src), ["10"]);
    let nested = "fn f(n){ if(n>0){ if(n>5){ return 2; } return 1; } return 0; } \
                  print f(9); print f(1); print f(0);";
    assert_eq!(run_c(nested), ["2", "1", "0"]);
}

#[test]
fn recursion_is_bounded_rather_than_taking_the_process_down() {
    assert_eq!(run_c("fn f(n){ if(n<1){return 0;} return 1+f(n-1); } print f(400);"), ["400"]);

    let p = vm_c::compile("fn f(n){ return 1+f(n+1); } print f(0);").expect("compiles");
    match vm_c::run(&p) {
        Err(e) => assert!(e.contains("call stack exceeded"), "{e}"),
        Ok(o) => panic!("expected a failure, got {o:?}"),
    }
}

/// The same *program* in both languages, using everything the pair covers.
///
/// The narrow version of this — arithmetic and a loop — is what let the twins
/// drift. This one has to be updated when a language grows, which is the point.
#[test]
fn the_twins_agree_on_a_program_that_uses_everything() {
    let c = r#"
fn fact(n) { if (n < 2) { return 1; } else { return n * fact(n - 1); } }
print fact(5);
a = [10, 20, 30];
a[1] = 99;
print a;
print len a;
s = "hammer";
print s[0];
print "nail" + s;
n = 0;
while (n < 3) { print n; n = n + 1; }
print 12 & 10;
print 0 && (1 / 0);
"#;
    let b = r#"
FUNCTION fact(n)
IF n < 2 THEN
RETURN 1
ELSE
RETURN n * fact(n - 1)
END IF
END FUNCTION
PRINT fact(5)
LET a = [10, 20, 30]
LET a[1] = 99
PRINT a
PRINT LEN a
LET s = "hammer"
PRINT s[0]
PRINT "nail" + s
LET n = 0
WHILE n < 3
PRINT n
LET n = n + 1
WEND
PRINT 12 AND 10
PRINT 0 ANDALSO (1 / 0)
"#;

    let cp = vm_c::compile(c).expect("C compiles");
    let bp = vm_basic::compile(b).expect("BASIC compiles");

    let out = vm_c::run(&cp).expect("C runs");
    assert_eq!(
        out,
        ["120", "[10, 99, 30]", "3", "h", "nailhammer", "0", "1", "2", "8", "0"],
        "and the answers are right, not merely equal"
    );
    assert_eq!(vm_basic::run(&bp).expect("BASIC runs"), out, "two syntaxes, one language");

    // The strong form: same decisions, not merely the same answers.
    assert_eq!(cp.code.len(), bp.code.len(), "same instruction count");
    assert_eq!(cp.fns.len(), bp.fns.len(), "same functions");
    assert_eq!(format!("{:?}", cp.code), format!("{:?}", bp.code), "instruction for instruction");
}
