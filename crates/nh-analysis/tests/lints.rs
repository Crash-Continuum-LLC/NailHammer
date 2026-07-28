//! Lint tests.
//!
//! Two properties matter equally: the lints **catch** real hazards, and they
//! **stay silent** on grammars that are fine. The second is not a nicety — a
//! determinism warning that fires spuriously is one people learn to ignore, and
//! then it protects nobody.

use nh_analysis::analyse;
use nh_syntax::{resolve, Severity, SourceMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(rel)
}

/// Analyses a grammar, returning `(severity, message)` per diagnostic.
fn lints_of(source: &str) -> Vec<(Severity, String)> {
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join("nh-analysis-tests");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!(
        "g{}-{}.nh",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, source).unwrap();

    let mut sm = SourceMap::new();
    let ast = resolve(&mut sm, &path).unwrap_or_else(|e| panic!("{}", e.render(&sm)));
    analyse(&ast, None)
        .into_iter()
        .map(|d| (d.severity, d.message))
        .collect()
}

fn fires(source: &str, needle: &str) -> bool {
    lints_of(source).iter().any(|(_, m)| m.contains(needle))
}

const PRELUDE: &str = "grammar T;\nskip WS = \" \";\ntoken ALPHA = @ \"a\"..\"z\";\ntoken IDENT = @ ALPHA+;\n";

fn g(rules: &str) -> String {
    format!("{PRELUDE}{rules}")
}

// ---------------------------------------------------------------------------
// Catching real hazards
// ---------------------------------------------------------------------------

#[test]
fn direct_left_recursion_is_an_error() {
    let out = lints_of(&g("rule e = e \"+\" IDENT -> add | IDENT -> lit;\n"));
    let (sev, msg) = out
        .iter()
        .find(|(_, m)| m.contains("left-recursive"))
        .expect("left recursion must be reported");
    assert_eq!(*sev, Severity::Error, "left recursion is fatal in a PEG");
    assert!(msg.contains("`e`"), "{msg}");
}

#[test]
fn indirect_left_recursion_is_an_error() {
    // a -> b -> a, with nothing consumed in between.
    assert!(fires(
        &g("rule a = b \"x\" -> one;\nrule b = a -> two | IDENT -> three;\n"),
        "left-recursive"
    ));
}

/// A reference is only *leading* if everything before it can match empty, so
/// recursion after a literal is ordinary recursion and must not be reported.
#[test]
fn recursion_behind_a_literal_is_not_left_recursion() {
    assert!(!fires(
        &g("rule a = \"(\" a \")\" -> nested | IDENT -> lit;\n"),
        "left-recursive"
    ));
}

#[test]
fn a_nullable_repetition_is_an_error() {
    assert!(fires(&g("rule r = (IDENT?)* -> spin;\n"), "never terminates"));
    assert!(fires(&g("rule r = (IDENT*)+ -> spin;\n"), "never terminates"));
}

#[test]
fn an_ordinary_repetition_is_fine() {
    assert!(!fires(&g("rule r = IDENT* -> many;\n"), "never terminates"));
    assert!(!fires(&g("rule r = (IDENT \",\")* -> many;\n"), "never terminates"));
}

#[test]
fn a_shadowed_alternative_is_reported() {
    assert!(fires(
        &g("rule kw = \"let\" -> short | \"letter\" -> long;\n"),
        "unreachable"
    ));
}

/// The reverse order is correct and must be silent.
#[test]
fn the_longer_alternative_first_is_fine() {
    assert!(!fires(
        &g("rule kw = \"letter\" -> long | \"let\" -> short;\n"),
        "unreachable"
    ));
}

/// `"a" X | "ab" Y` is NOT shadowing: if `X` fails the whole alternative fails
/// and the PEG backtracks into the second. Reporting it would be a false alarm.
#[test]
fn a_prefix_followed_by_more_is_not_shadowing() {
    assert!(!fires(
        &g("rule kw = \"let\" IDENT -> a | \"letter\" IDENT -> b;\n"),
        "unreachable"
    ));
}

#[test]
fn shadowing_respects_case_folding() {
    let src = "grammar T;\nkeywords case-insensitive;\nskip WS = \" \";\n\
               rule kw = \"LET\" -> short | \"letter\" -> long;\n";
    assert!(fires(src, "unreachable"), "folded literals still shadow");
}

#[test]
fn an_alternative_that_matches_empty_hides_the_rest() {
    let out = lints_of(&g("rule r = IDENT? -> maybe | \"x\" -> lit;\n"));
    let (sev, _) = out
        .iter()
        .find(|(_, m)| m.contains("can match nothing"))
        .expect("must be reported");
    assert_eq!(*sev, Severity::Error);
}

#[test]
fn an_empty_matching_alternative_last_is_fine() {
    assert!(!fires(
        &g("rule r = \"x\" -> lit | IDENT? -> maybe;\n"),
        "can match nothing"
    ));
}

#[test]
fn a_binding_repeated_in_one_sequence_is_reported() {
    assert!(fires(
        &g("rule r = a:IDENT \",\" a:IDENT -> pair;\n"),
        "bound twice"
    ));
}

/// Binding the same name in two branches of a choice is correct: only one
/// branch matches. Reporting it would be a false positive.
#[test]
fn the_same_binding_in_two_branches_is_fine() {
    assert!(!fires(
        &g("rule r = (\"a\" v:IDENT | \"b\" v:IDENT) -> pick;\n"),
        "bound twice"
    ));
}

#[test]
fn a_repeated_binding_is_fine() {
    assert!(!fires(&g("rule r = items:IDENT* -> many;\n"), "bound twice"));
}

#[test]
fn unused_rules_and_tokens_are_reported() {
    let src = g("rule entry = IDENT -> lit;\nrule orphan = \"x\" -> dead;\n");
    assert!(fires(&src, "rule `orphan` is never referenced"));
    // The first rule is the conventional entry point and is exempt.
    assert!(!fires(&src, "rule `entry` is never referenced"));
}

/// A sync point that can match empty makes the generated error node
/// unmatchable, so recovery silently does nothing.
#[test]
fn a_nullable_sync_point_is_an_error() {
    let out = lints_of(&g(
        "rule stmt = IDENT \";\" -> s;\nrecover stmt sync \";\"?;\n",
    ));
    let (sev, _) = out
        .iter()
        .find(|(_, m)| m.contains("sync point"))
        .expect("must be reported");
    assert_eq!(*sev, Severity::Error);
}

#[test]
fn a_concrete_sync_point_is_fine() {
    assert!(!fires(
        &g("rule stmt = IDENT \";\" -> s;\nrecover stmt sync \";\";\n"),
        "sync point"
    ));
}

// ---------------------------------------------------------------------------
// Suppression
// ---------------------------------------------------------------------------

#[test]
fn allow_silences_a_lint_for_one_rule() {
    let src = g("rule kw = \"let\" -> short | \"letter\" -> long;\n");
    assert!(fires(&src, "unreachable"));

    let silenced = format!("{src}allow shadow in kw;\n");
    assert!(!fires(&silenced, "unreachable"), "`allow` must silence it");
}

#[test]
fn allow_is_scoped_to_the_named_rule() {
    let src = g(
        "rule kw = \"let\" -> a | \"letter\" -> b;\n\
         rule other = \"do\" -> c | \"double\" -> d;\n\
         allow shadow in kw;\n",
    );
    let messages: Vec<String> = lints_of(&src).into_iter().map(|(_, m)| m).collect();
    let shadows = messages.iter().filter(|m| m.contains("unreachable")).count();
    assert_eq!(shadows, 1, "only `other` should still report: {messages:?}");
}

/// An `allow` naming a lint that does not exist silences nothing, and the
/// author believes they are covered. That is worse than not writing it.
#[test]
fn an_unknown_lint_name_is_an_error() {
    let out = lints_of(&g("rule r = IDENT -> lit;\nallow nonsense in r;\n"));
    let (sev, msg) = out
        .iter()
        .find(|(_, m)| m.contains("unknown lint"))
        .expect("must be reported");
    assert_eq!(*sev, Severity::Error);
    assert!(msg.contains("nonsense"), "{msg}");
}

// ---------------------------------------------------------------------------
// No false positives on real grammars
// ---------------------------------------------------------------------------

/// Every grammar shipped with the repo must analyse clean.
///
/// This is the test that keeps the lints honest: it is easy to write a check
/// that finds hazards, and hard to write one that does not also flag working
/// code.
#[test]
fn the_shipped_grammars_produce_no_diagnostics() {
    for rel in [
        "example.nh",
        "examples/calc.nh",
        "examples/basic.nh",
        "examples/config/config.nh",
        "examples/calc-interp/calc.nh",
    ] {
        let mut sm = SourceMap::new();
        let ast = resolve(&mut sm, &repo(rel)).unwrap_or_else(|e| panic!("{}", e.render(&sm)));
        let table = nh_operators::resolve(&ast, &mut sm).unwrap();

        let found = analyse(&ast, table.atom_rule.as_deref());
        assert!(
            found.is_empty(),
            "{rel} should analyse clean, got:\n{}",
            found
                .iter()
                .map(|d| format!("  {}: {}", d.severity.label(), d.message))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

// ---------------------------------------------------------------------------
// Silent rules
// ---------------------------------------------------------------------------

const SILENT: &str = "grammar S;\nskip WS = \" \";\ntoken ALPHA = @ \"a\"..\"z\";\n\
                      token IDENT = @ ALPHA+;\nrule top = SOI item* EOI;\n\
                      silent rule item = a | b;\nrule a = \"a\" n:IDENT -> ay;\n\
                      rule b = \"b\" n:IDENT -> bee;\n";

/// A silent rule produces no node, so a binding onto it has nothing to attach
/// to. Pest rejects it, but its message points at generated `.pest` and names
/// no grammar line — so this must be caught earlier.
#[test]
fn binding_a_silent_rule_is_an_error() {
    let src = SILENT.replace("rule top = SOI item* EOI;", "rule top = SOI thing:item EOI -> t;");
    let out = lints_of(&src);
    let (sev, msg) = out
        .iter()
        .find(|(_, m)| m.contains("silent"))
        .expect("must be reported");
    assert_eq!(*sev, Severity::Error);
    assert!(msg.contains("`thing`") && msg.contains("`item`"), "{msg}");
}

/// The same problem through a repetition.
#[test]
fn binding_a_repeated_silent_rule_is_an_error() {
    let src = SILENT.replace("rule top = SOI item* EOI;", "rule top = SOI things:item* EOI -> t;");
    assert!(fires(&src, "produces no node"));
}

/// Referencing a silent rule without binding it is the normal case.
#[test]
fn referencing_a_silent_rule_is_fine() {
    assert!(!fires(SILENT, "produces no node"));
}

/// Bindings *inside* a silent rule are fine — its children still appear.
#[test]
fn a_binding_inside_a_silent_rule_is_fine() {
    let src = SILENT.replace("silent rule item = a | b;", "silent rule item = n:IDENT | a | b;");
    assert!(!fires(&src, "produces no node"));
}
