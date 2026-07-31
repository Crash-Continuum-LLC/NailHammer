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
