//! `nh trace` — what a program routes to, without compiling anything.
//!
//! The point of these is not that the renderer produces particular characters.
//! It is that the three things a person opens this to find out are actually
//! there: which handler, what it receives, and which of those have *not* been
//! evaluated yet.

use std::path::PathBuf;
use std::process::Command;

fn nh() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nh"))
}

fn repo(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

/// Scaffolds a grammar to trace against. Only the `.nh` is used, so this is
/// cheap — no cargo, no generated Rust.
fn grammar(name: &str, extra: &[&str]) -> PathBuf {
    let dir = std::env::temp_dir().join("nh-trace-tests").join(format!(
        "{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let out = nh()
        .args(["init", dir.to_str().unwrap(), "--name", name])
        .args(extra)
        .output()
        .expect("running nh init");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));

    dir.join(format!("{name}.nh"))
}

fn trace(g: &std::path::Path, source: &str, extra: &[&str]) -> String {
    let out = nh()
        .arg("trace")
        .arg(g)
        .args(["--source", source])
        .args(extra)
        .output()
        .expect("running nh trace");
    assert!(
        out.status.success(),
        "`nh trace` failed on {source:?}:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

// ---------------------------------------------------------------------------

/// The three questions, in one program.
#[test]
fn a_trace_names_the_handler_its_arguments_and_their_values() {
    let g = grammar("t1", &[]);
    let out = trace(&g, "let width = 4;", &[]);

    assert!(out.contains("stmt_bind"), "which handler:\n{out}");
    assert!(
        out.contains("handlers/stmt_bind.rs"),
        "and the file to open:\n{out}"
    );
    assert!(out.contains("name"), "the parameter names:\n{out}");
    assert!(out.contains("\"width\""), "and a token's actual text:\n{out}");
    assert!(
        out.contains("primary_num"),
        "including what produced the other argument:\n{out}"
    );
}

/// The distinction people get wrong, and the reason this exists.
#[test]
fn lazy_arguments_are_marked_as_not_yet_evaluated() {
    let g = grammar("t2", &[]);
    let out = trace(&g, "if x { print 1; }", &[]);

    let then = out
        .lines()
        .find(|l| l.trim_start().starts_with("then:"))
        .unwrap_or_else(|| panic!("no `then` argument:\n{out}"));
    assert!(
        then.contains("lazy"),
        "`then` is lazy — the handler gets the node, not a value:\n{then}"
    );

    let cond = out
        .lines()
        .find(|l| l.trim_start().starts_with("cond:"))
        .unwrap_or_else(|| panic!("no `cond` argument:\n{out}"));
    assert!(
        !cond.contains("lazy") && cond.contains("evaluated first"),
        "`cond` is eager, and the contrast is the whole point:\n{cond}"
    );
}

/// An optional binding that was not used is *absent*, not lazy and not empty.
#[test]
fn an_optional_binding_that_did_not_match_says_so() {
    let g = grammar("t3", &[]);
    let with = trace(&g, "if x { print 1; } else { print 2; }", &[]);
    let without = trace(&g, "if x { print 1; }", &[]);

    let line = |s: &str| {
        s.lines()
            .find(|l| l.trim_start().starts_with("otherwise:"))
            .map(str::to_string)
            .unwrap_or_default()
    };
    assert!(line(&without).contains("absent"), "{}", line(&without));
    assert!(!line(&with).contains("absent"), "{}", line(&with));
}

/// Operators do not route to a handler. Saying which role they bind is the
/// only way to find out where `+` actually goes.
#[test]
fn operators_are_reported_as_roles_not_handlers() {
    let g = grammar("t4", &[]);
    let out = trace(&g, "print 2 + 3 * 4;", &[]);

    assert!(out.contains("Operators::add"), "`+` binds `add`:\n{out}");
    assert!(out.contains("Operators::mul"), "`*` binds `mul`:\n{out}");
    assert!(
        !out.contains("handlers/add.rs"),
        "there is no handler for an operator:\n{out}"
    );
}

/// Arguments own their subtrees. A flat list would show the condition and the
/// body as siblings and leave you to guess which was which.
#[test]
fn a_subtree_hangs_off_the_argument_it_produces() {
    let g = grammar("t5", &[]);
    let out = trace(&g, "if x { print 1; }", &[]);

    let lines: Vec<&str> = out.lines().collect();
    // Matching on the trimmed *start* matters: the grammar-source line under
    // each handler also contains `cond:expr` and `then:block`.
    let at = |needle: &str| {
        lines
            .iter()
            .position(|l| l.trim_start().starts_with(needle))
            .unwrap_or_else(|| panic!("no `{needle}`:\n{out}"))
    };
    let indent = |i: usize| lines[i].len() - lines[i].trim_start().len();

    // `primary_var` produces `cond`, so it must be nested under it — and under
    // `then`, not beside it.
    assert!(at("cond:") < at("primary_var"), "{out}");
    assert!(at("primary_var") < at("then:"), "{out}");
    assert!(
        indent(at("primary_var")) > indent(at("cond:")),
        "the producer nests under its argument:\n{out}"
    );
}

/// The line-oriented style traces too — this is grammar-driven, not hardcoded.
#[test]
fn the_other_syntax_style_traces_as_well() {
    let g = grammar("t6", &["--style", "basic", "--with", "loops"]);
    let out = trace(&g, "WHILE x\n  PRINT 1\nWEND\n", &[]);

    assert!(out.contains("stmt_while"), "{out}");
    let cond = out
        .lines()
        .find(|l| l.trim_start().starts_with("cond:"))
        .unwrap_or_else(|| panic!("no `cond`:\n{out}"));
    assert!(
        cond.contains("lazy"),
        "a loop condition is re-tested, so it must be lazy:\n{cond}"
    );
}

/// The editor needs the same tree as data.
#[test]
fn json_carries_everything_the_text_does() {
    let g = grammar("t7", &[]);
    let out = trace(&g, "if x { print 1 + 2; }", &["--json"]);

    for key in [
        "\"handler\"",
        "\"args\"",
        "\"name\"",
        "\"ty\"",
        "\"text\"",
        "\"lazy\"",
        "\"matched\"",
        "\"operators\"",
        "\"from\"",
    ] {
        assert!(out.contains(key), "missing {key}:\n{out}");
    }
    assert!(out.contains("\"role\":\"add\""), "{out}");
    assert!(out.starts_with('{') && out.trim_end().ends_with('}'), "{out}");
}

/// A statement `recover` got past routes **nowhere**, and the trace has to say
/// so.
///
/// This test was written expecting a failure and found something better: with
/// `recover stmt sync ";"` the parse *succeeds*, so the bad statement was
/// silently absent from the trace — leaving exactly the wrong impression, that
/// it had been handled.
#[test]
fn a_recovered_statement_is_shown_as_going_nowhere() {
    let g = grammar("t8", &[]);
    let out = trace(&g, "@@@ ;\nprint 1;\n", &[]);

    assert!(
        out.contains("did not parse"),
        "a recovered statement must be visible:\n{out}"
    );
    assert!(
        out.contains("no handler runs"),
        "and it must say that nothing handles it:\n{out}"
    );
    assert!(
        out.contains("stmt_print"),
        "while the good statement after it still traces:\n{out}"
    );
}

/// It works on the grammars this repository ships, which are not scaffolds.
#[test]
fn the_worked_examples_trace() {
    let out = nh()
        .arg("trace")
        .arg(repo("examples/basic-interp/basic.nh"))
        .args(["--source", "10 PRINT 1\n20 GOTO 10\n"])
        .output()
        .expect("running nh trace");

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("stmt_goto"), "{s}");
    assert!(s.contains("target"), "and its argument:\n{s}");
}
