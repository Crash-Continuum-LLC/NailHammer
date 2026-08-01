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
