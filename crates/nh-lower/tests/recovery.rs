//! Recovery must not eat the thing that closes the construct it is inside.
//!
//! # The bug
//!
//! `recover stmt sync ";"` lowered to `(!(";") ~ ANY)+`, which consumes
//! anything that is not a `;` — including the `}` that closes the block the
//! statement is inside. At the closing brace, `stmt`'s real body fails, the
//! error node matches, and it eats the brace. `stmt*` never terminates and the
//! block never closes.
//!
//! It broke **every** grammar with a block. The examples in this repository
//! escaped only because they recover at the top level, where the closer is
//! `EOI` and `ANY` stops there anyway — so nothing caught it until `nh init`
//! grew an `if` with a braced body.
//!
//! What the user saw was a parse error pointing at the `if`, with nothing
//! anywhere mentioning recovery.

use nh_lower::{lower, Lowered};
use nh_syntax::{resolve, SourceMap};
use pest_vm::Vm;

fn build_str(source: &str) -> Lowered {
    let dir = std::env::temp_dir().join("nh-recovery-tests");
    std::fs::create_dir_all(&dir).unwrap();
    let path = {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        dir.join(format!(
            "g{}_{}.nh",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ))
    };
    std::fs::write(&path, source).unwrap();

    let mut sm = SourceMap::new();
    let ast = match resolve(&mut sm, &path) {
        Ok(a) => a,
        Err(e) => panic!("parsing failed:\n{}", e.render(&sm)),
    };
    let table = nh_operators::resolve(&ast, &mut sm)
        .unwrap_or_else(|e| panic!("operator table failed:\n{}", e.render(&sm)));
    lower(&ast, &table).unwrap_or_else(|e| panic!("lowering failed:\n{}", e.render(&sm)))
}

/// A C-flavoured grammar with a braced block — the shape that was broken.
const BRACED: &str = r#"
grammar Blk;
use operators::core;
skip WS = " " | "\t" | "\r" | "\n";
token DIGIT = @ "0".."9";
token ALPHA = @ "a".."z";
token NUMBER = @ DIGIT+;
token IDENT = @ ALPHA+;
reserved from IDENT { "if" "else" "print" }
rule program = SOI stmts:stmt* EOI -> doc;
rule block = "{" stmts:stmt* "}" -> body;
rule stmt
  = "print" value:expr ";"                                   -> print
  | "if" cond:expr lazy then:block lazy otherwise:else_tail? -> iff
  | value:expr ";"                                           -> eval
  ;
rule else_tail = "else" lazy body:block -> tail;
rule atom = primary;
rule primary = digits:NUMBER -> num | name:IDENT -> var place | "(" inner:expr ")" -> pass;
recover stmt sync ";";
"#;

/// A line-oriented grammar, where the closer is a keyword three rules away.
const LINE_ORIENTED: &str = r#"
grammar Lin;
use operators::none;
precedence { left "+" ; atom atom; }
skip WS = " " | "\t";
token EOL = @ "\r"? "\n";
token DIGIT = @ "0".."9";
token ALPHA = @ "a".."z";
token NUMBER = @ DIGIT+;
token IDENT = @ ALPHA+;
reserved from IDENT { "while" "wend" "print" }
rule program = SOI EOL* stmts:line* EOI -> doc;
rule line = body:stmt EOL* -> one;
rule block = stmts:line* -> body;
rule stmt
  = "while" lazy cond:expr EOL* lazy body:block "wend" -> while
  | "print" value:expr                                 -> print
  | value:expr                                         -> eval
  ;
rule atom = primary;
rule primary = digits:NUMBER -> num | name:IDENT -> var;
recover stmt sync EOL;
"#;

/// Every rule name appearing in the parse of `text`.
///
/// Asserting on the *tree* rather than on "did it parse" is the whole lesson of
/// this file: with the bug present, `program` still parsed. Recovery swallowed
/// the entire program into one error node and the top-level `stmt*` was
/// perfectly happy. A test that only checked for success passed against the bug.
fn rules_in(l: &Lowered, text: &str) -> Vec<String> {
    let v = vm(l);
    let pairs = v
        .parse("program", text)
        .unwrap_or_else(|e| panic!("did not parse:\n{e}\n\n{}", l.pest));
    pairs.flatten().map(|p| p.as_rule().to_string()).collect()
}

fn vm(l: &Lowered) -> Vm {
    Vm::new(
        pest_meta::parse_and_optimize(&l.pest)
            .unwrap_or_else(|e| panic!("generated pest is invalid: {e:?}\n{}", l.pest))
            .1,
    )
}

// ---------------------------------------------------------------------------
// The regression itself
// ---------------------------------------------------------------------------

/// Without the fix this failed: the error node ate the `}`.
#[test]
fn a_braced_block_closes() {
    let l = build_str(BRACED);
    let rules = rules_in(&l, "if a { print 1; }\n");

    assert!(
        rules.iter().any(|r| r == "stmt_iff"),
        "the `if` must be parsed as an `if`, not recovered:\n{rules:?}"
    );
    assert!(
        rules.iter().any(|r| r == "block"),
        "and its body as a block:\n{rules:?}"
    );
    assert!(
        !rules.iter().any(|r| r == "nh_error_stmt"),
        "nothing here is an error:\n{rules:?}"
    );
}

#[test]
fn a_keyword_closed_block_closes() {
    let l = build_str(LINE_ORIENTED);
    let rules = rules_in(&l, "while a\nprint 1\nwend\n");

    assert!(
        rules.iter().any(|r| r == "stmt_while"),
        "the `while` must be parsed as a loop, not recovered:\n{rules:?}"
    );
    assert!(
        !rules.iter().any(|r| r == "nh_error_stmt"),
        "nothing here is an error:\n{rules:?}"
    );
}

/// The closers must be *derived*, not assumed to be one level up. In the
/// line-oriented grammar the chain is `stmt` -> `line` -> `block` -> `wend`.
#[test]
fn a_closer_is_found_through_intervening_rules() {
    let l = build_str(LINE_ORIENTED);
    let err = l
        .pest
        .lines()
        .find(|line| line.starts_with("nh_error_stmt"))
        .expect("an error rule");
    assert!(
        err.contains("nh_kw_wend"),
        "`wend` closes the block two rules above `stmt`:\n{err}"
    );
}

/// A reserved word keeps its boundary guard, so a variable that merely starts
/// with one is not a stopping point.
#[test]
fn a_word_closer_is_guarded_not_a_bare_literal() {
    let l = build_str(LINE_ORIENTED);
    let err = l
        .pest
        .lines()
        .find(|line| line.starts_with("nh_error_stmt"))
        .expect("an error rule");
    assert!(
        !err.contains(r#"!("wend")"#),
        "a bare literal would also stop at `wendy`:\n{err}"
    );
}

// ---------------------------------------------------------------------------
// What must not change
// ---------------------------------------------------------------------------

/// Recovery still recovers. This is the whole point of the feature, and a fix
/// that stopped it would be worse than the bug.
#[test]
fn recovery_still_skips_a_bad_statement() {
    let l = build_str(BRACED);

    let rules = rules_in(&l, "@@@ ;\nprint 1;\n");
    assert!(
        rules.iter().any(|r| r == "nh_error_stmt"),
        "the garbage should have been recovered:\n{rules:?}"
    );
    assert!(
        rules.iter().any(|r| r == "stmt_print"),
        "and the good statement after it still parsed:\n{rules:?}"
    );

    // Inside a block: recovery works *and* the block still closes.
    let rules = rules_in(&l, "if a { @@@ ; print 1; }\n");
    assert!(
        rules.iter().any(|r| r == "nh_error_stmt"),
        "recovery must reach inside a block:\n{rules:?}"
    );
    assert!(
        rules.iter().any(|r| r == "stmt_iff") && rules.iter().any(|r| r == "stmt_print"),
        "and the block must still close around it:\n{rules:?}"
    );
}

/// A grammar whose recovered rule is never inside anything gains no guards, so
/// the fix costs nothing where it was not needed.
#[test]
fn a_top_level_only_grammar_is_unchanged() {
    let l = build_str(
        r#"
grammar Flat;
use operators::core;
skip WS = " " | "\t" | "\r" | "\n";
token DIGIT = @ "0".."9";
token NUMBER = @ DIGIT+;
rule program = SOI stmts:stmt* EOI -> doc;
rule stmt = value:expr ";" -> eval;
rule atom = primary;
rule primary = digits:NUMBER -> num;
recover stmt sync ";";
"#,
    );
    let err = l
        .pest
        .lines()
        .find(|line| line.starts_with("nh_error_stmt"))
        .expect("an error rule");
    assert_eq!(
        err, r#"nh_error_stmt = { (!(";") ~ ANY)+ ~ (";")? }"#,
        "nothing encloses `stmt`, so nothing should be excluded"
    );
}

/// The sync token is not repeated as a stop. It is already the first guard, and
/// a second copy would be noise in a file people read.
#[test]
fn the_sync_token_is_not_listed_twice() {
    let l = build_str(LINE_ORIENTED);
    let err = l
        .pest
        .lines()
        .find(|line| line.starts_with("nh_error_stmt"))
        .expect("an error rule");
    assert_eq!(err.matches("!(EOL)").count(), 1, "{err}");
}
