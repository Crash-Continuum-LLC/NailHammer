//! What the CLI *tells* you, as opposed to what it computes.
//!
//! Every test here covers a defect where the tool worked correctly and then
//! failed to say so, or said it two different ways. That class does not show up
//! in a test of the computation, because the computation was never wrong — the
//! table really was replaced, the precedence really was resolved. What broke was
//! the sentence about it, and a wrong sentence is what the user acts on.

use std::path::PathBuf;
use std::process::Command;

fn nh() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nh"))
}

/// Writes a grammar to a scratch file. Only `check`, `explain` and `trace` run
/// against it, so nothing is compiled and this stays cheap.
fn grammar(name: &str, body: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("nh-reporting-tests")
        .join(format!("{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.nh"));
    std::fs::write(&path, body).unwrap();
    path
}

/// A three-tier table with one operator per tier, so a precedence number can be
/// checked against a known position rather than against another number that
/// might be wrong the same way.
const THREE_TIERS: &str = r#"grammar Prec;
use operators::none;
skip WS = " " | "\t" | "\n";
token DIGIT = @ "0".."9";
token NUMBER = @ DIGIT+;
token ALPHA = @ "a".."z";
token IDENT = @ ALPHA+;
precedence {
    left  "+";
    left  "*";
    right "^" -> pow;
    atom atom;
}
rule program = SOI expr EOI;
rule atom = primary;
rule primary = value:NUMBER -> num | name:IDENT -> var place;
"#;

/// The usage text is the first thing anyone reads, and it listed `nh trace`
/// twice — two identical lines, which reads as though there are two forms of
/// the command and invites a hunt for the difference.
#[test]
fn the_usage_text_lists_each_command_once() {
    let out = nh().arg("--help").output().expect("running nh --help");
    let text = String::from_utf8_lossy(&out.stdout) + String::from_utf8_lossy(&out.stderr);

    let usage: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("nh "))
        .collect();

    let mut seen = usage.clone();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(
        seen.len(),
        usage.len(),
        "a usage line is repeated:\n{}",
        usage.join("\n")
    );
}

/// Replacing a preset by accident is the most destructive thing a `precedence`
/// block can do, and it did it silently: the warning existed but was dropped on
/// the success path, so `nh check` printed a clean `ok:` over a table that had
/// lost thirty operators.
#[test]
fn check_reports_that_a_bare_block_discarded_the_preset() {
    let path = grammar(
        "discard",
        &THREE_TIERS.replace("use operators::none;", "use operators::c_style;"),
    );

    let out = nh().arg("check").arg(&path).output().expect("running nh check");
    let text = String::from_utf8_lossy(&out.stderr);

    assert!(
        text.contains("replaces the preset"),
        "the warning must reach the user:\n{text}"
    );
    assert!(
        out.status.success(),
        "it is a warning, not an error — the grammar is still legal"
    );
}

/// The correct spelling must stay quiet, or the warning above becomes noise
/// people filter out.
#[test]
fn check_stays_quiet_when_the_block_says_override() {
    let path = grammar(
        "override",
        "grammar Ovr;\nuse operators::c_style;\n\
         skip WS = \" \" | \"\\t\" | \"\\n\";\n\
         token DIGIT = @ \"0\"..\"9\";\ntoken NUMBER = @ DIGIT+;\n\
         precedence override {\n  right \"**\" above \"*\" -> pow;\n}\n\
         rule program = SOI expr EOI;\nrule atom = primary;\n\
         rule primary = value:NUMBER -> num;\n",
    );

    let out = nh().arg("check").arg(&path).output().expect("running nh check");
    let text = String::from_utf8_lossy(&out.stderr);

    assert!(
        !text.contains("replaces the preset"),
        "override adjusts rather than replaces:\n{text}"
    );
}

/// `nh explain` and `nh trace` describe the same tiers and disagreed about the
/// numbers: explain counted down from the loosest, trace printed the raw index
/// counting up. The same `*` was "precedence 2" in one and "precedence 1" in the
/// other, which makes the two commands unusable together.
#[test]
fn explain_and_trace_agree_about_precedence() {
    let path = grammar("agree", THREE_TIERS);

    let explained = nh().arg("explain").arg(&path).output().expect("running nh explain");
    let explained = String::from_utf8_lossy(&explained.stdout).into_owned();

    let traced = nh()
        .args(["trace"])
        .arg(&path)
        .args(["--source", "a + b * c"])
        .output()
        .expect("running nh trace");
    let traced = String::from_utf8_lossy(&traced.stdout).into_owned();

    for (op, _) in [("+", 3usize), ("*", 2)] {
        // explain: the leading column of the line naming the operator.
        let from_explain: usize = explained
            .lines()
            .find(|l| l.split_whitespace().nth(1) == Some(op))
            .unwrap_or_else(|| panic!("no explain line for `{op}`:\n{explained}"))
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap();

        // trace: "`+` — left-associative, precedence 3"
        let from_trace: usize = traced
            .lines()
            .find(|l| l.contains(&format!("`{op}`")) && l.contains("precedence"))
            .unwrap_or_else(|| panic!("no trace line for `{op}`:\n{traced}"))
            .rsplit_once("precedence ")
            .unwrap()
            .1
            .trim()
            .parse()
            .unwrap();

        assert_eq!(
            from_explain, from_trace,
            "`{op}`: explain says {from_explain}, trace says {from_trace}"
        );
    }
}

/// The direction is part of the contract, not an accident of storage: a reader
/// coming from a C precedence chart expects the tightest tier to be 1.
#[test]
fn the_tightest_tier_prints_as_one() {
    let path = grammar("direction", THREE_TIERS);
    let out = nh().arg("explain").arg(&path).output().expect("running nh explain");
    let text = String::from_utf8_lossy(&out.stdout);

    let numbered = |op: &str| -> usize {
        text.lines()
            .find(|l| l.split_whitespace().nth(1) == Some(op))
            .unwrap_or_else(|| panic!("no line for `{op}`:\n{text}"))
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap()
    };

    assert_eq!(numbered("^"), 1, "tightest is 1");
    assert!(numbered("+") > numbered("*"), "looser prints higher");
}

/// `--json` covers every outcome, not only the ones that got as far as the
/// lints.
///
/// An editor asks for JSON and parses stdout. Errors from resolving or lowering
/// stopped the pipeline *before* the `--json` branch and were rendered to
/// stderr as text, so a grammar with a real error put **nothing** in the
/// Problems panel — while a grammar with only warnings filled it. The one case
/// the panel exists for was the one case it could not show.
#[test]
fn a_grammar_error_reaches_json_too() {
    let g = grammar(
        "jsonerr",
        "grammar J;\nuse operators::none;\nrule program = SOI body:missing_thing EOI -> prog;\n",
    );
    let out = nh()
        .args(["check", g.to_str().unwrap(), "--json"])
        .output()
        .expect("running nh check");

    assert!(!out.status.success(), "an undefined reference is an error");
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        s.starts_with('[') && s.trim_end().ends_with(']'),
        "stdout must be the JSON array and nothing else:\n{s}"
    );
    assert!(s.contains("\"severity\":\"error\""), "{s}");
    assert!(s.contains("missing_thing"), "{s}");
    // The location is the point of the exercise — a diagnostic an editor
    // cannot place is one it cannot show.
    assert!(s.contains("\"line\":3"), "{s}");
}

/// The same for a diagnostic raised while lowering rather than resolving, since
/// those travel a different path out.
#[test]
fn a_lowering_error_reaches_json_too() {
    let g = grammar(
        "jsonlower",
        "grammar K;\nuse operators::none;\nskip WS = \" \";\n\
         token ALPHA = @ \"a\"..\"z\";\ntoken ID = @ ALPHA+;\n\
         rule program = SOI body:many EOI -> prog;\n\
         rule many = stmt*;\nrule stmt = v:ID -> s;\n",
    );
    let out = nh()
        .args(["check", g.to_str().unwrap(), "--json"])
        .output()
        .expect("running nh check");

    assert!(!out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("\"severity\":\"error\""), "{s}");
    assert!(s.contains("produces 0 or more nodes"), "{s}");
}
