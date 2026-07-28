//! Integration tests for M0: parsing, rendering, and import resolution.

use nh_syntax::ast::{CaseMode, ExprKind, RepeatKind};
use nh_syntax::{render, resolve, Ast, SourceMap};
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

fn ok(path: &Path) -> Ast {
    let mut sm = SourceMap::new();
    match resolve(&mut sm, path) {
        Ok(ast) => ast,
        Err(e) => panic!("expected {} to parse:\n{}", path.display(), e.render(&sm)),
    }
}

fn err(path: &Path) -> String {
    let mut sm = SourceMap::new();
    match resolve(&mut sm, path) {
        Ok(_) => panic!("expected {} to fail", path.display()),
        Err(e) => e.render(&sm),
    }
}

/// Parses a literal grammar via a temp file, since `resolve` is path-based.
fn parse_str(body: &str) -> Result<Ast, String> {
    let dir = std::env::temp_dir().join("nh-tests");
    std::fs::create_dir_all(&dir).unwrap();
    let path = unique_path(&dir, ".nh");
    std::fs::write(&path, body).unwrap();

    let mut sm = SourceMap::new();
    resolve(&mut sm, &path).map_err(|e| e.render(&sm))
}

/// A path no other test can be writing.
///
/// This used to be a content hash, with a comment claiming it stopped
/// collisions. It caused them: two tests with *identical* grammar text got the
/// same path, and one truncated the file while the other was reading it. The
/// symptom was an occasional "no `grammar` declaration found" in whichever test
/// lost the race.
fn unique_path(dir: &std::path::Path, ext: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    dir.join(format!(
        "g{}_{}{ext}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ))
}


// ---------------------------------------------------------------------------
// The shipped examples
// ---------------------------------------------------------------------------

#[test]
fn example_parses() {
    let ast = ok(&repo("example.nh"));
    assert_eq!(ast.grammar_name.as_deref().map(String::as_str), Some("Example"));
    assert_eq!(ast.rules.len(), 4);
    assert_eq!(ast.uses.len(), 1);
}

#[test]
fn calc_merges_imported_lexical_fragment() {
    let ast = ok(&repo("examples/calc.nh"));

    // NUMBER, IDENT, STRING and the character classes come from common_lex.nh.
    let tokens: Vec<&str> = ast.tokens.iter().map(|t| t.name.value.as_str()).collect();
    for expected in ["DIGIT", "ALPHA", "ALNUM", "NUMBER", "IDENT", "STRING"] {
        assert!(tokens.contains(&expected), "missing imported token {expected}");
    }

    // The importing file's own definitions survive the merge.
    let rules: Vec<&str> = ast.rules.iter().map(|r| r.name.value.as_str()).collect();
    assert!(rules.contains(&"suffix"));

    // `precedence override` is recorded as an override, not a fresh table.
    assert_eq!(ast.precedence.len(), 1);
    assert!(ast.precedence[0].is_override);
}

#[test]
fn basic_carries_word_operators_and_case_folding() {
    let ast = ok(&repo("examples/basic.nh"));

    assert_eq!(ast.keywords_case.as_ref().map(|m| m.value), Some(CaseMode::Insensitive));

    // Identifier folding is the *other* knob, set per token (DESIGN.md §5.3).
    let ident = ast.tokens.iter().find(|t| t.name.value == "IDENT").unwrap();
    assert!(ident.case_insensitive, "BASIC folds identifiers too");
    assert!(ident.atomic);

    // The table is written from scratch, not an override of a preset.
    let block = &ast.precedence[0];
    assert!(!block.is_override);

    let words = block
        .entries
        .iter()
        .filter_map(|e| match e {
            nh_syntax::ast::PrecEntry::Op(op) => Some(op),
            _ => None,
        })
        .flat_map(|op| op.ops.iter())
        .filter(|o| o.word)
        .map(|o| o.literal.value.as_str())
        .collect::<Vec<_>>();
    for expected in ["OR", "XOR", "AND", "NOT", "MOD"] {
        assert!(words.contains(&expected), "missing word operator {expected}");
    }
}

// ---------------------------------------------------------------------------
// Regressions
// ---------------------------------------------------------------------------

/// Keyword rules must be atomic in nh.pest.
///
/// When they were silent (`_`), pest inserted implicit WHITESPACE between the
/// literal and its `!ident_cont` guard, so the guard tested the *following*
/// identifier, matched it, and failed the lookahead — silently breaking every
/// keyword-led rule. This is the canary for that.
#[test]
fn keyword_boundary_guard_survives_implicit_whitespace() {
    parse_str("grammar A;\n").expect("`grammar A;` must parse");
    parse_str("grammar A;\nrule x = y;\n").expect("`rule x = y;` must parse");
}

/// The same guard in the other direction: a keyword immediately followed by
/// identifier characters is not that keyword.
#[test]
fn keyword_does_not_match_a_longer_identifier() {
    assert!(
        parse_str("grammar A;\nrulex = y;\n").is_err(),
        "`rulex` must not lex as `rule x`"
    );
}

/// Tag lookup must scan direct children only.
///
/// `Pairs::find_first_tagged` is built on `.flatten()` and searches the whole
/// subtree, so an outer node with no `name` tag would find a *nested* binding's
/// tag and wrap the wrong expression. Here the outer `labeled` is unbound and
/// the inner one is bound; a flattening lookup would render `inner:(inner:X)*`.
#[test]
fn nested_binding_does_not_leak_to_the_enclosing_node() {
    let ast = parse_str("grammar A;\nrule r = (inner:X)*;\n").unwrap();
    let body = &ast.rules[0].alternatives[0].body;

    match &body.kind {
        ExprKind::Repeat { kind, inner } => {
            assert_eq!(*kind, RepeatKind::ZeroOrMore);
            assert!(
                matches!(inner.kind, ExprKind::Bind { .. }),
                "the binding belongs inside the repetition"
            );
        }
        other => panic!("expected a repetition at the top, got {other:?}"),
    }

    assert!(render(&ast).contains("(inner:X)*"));
}

/// `place` is only a marker after `-> label`; elsewhere it is an ordinary rule
/// reference. That is what makes the marker unambiguous (DESIGN.md §6.8).
#[test]
fn place_outside_an_arrow_is_a_rule_reference() {
    let ast = parse_str("grammar A;\nrule r = x place;\n").unwrap();
    let alt = &ast.rules[0].alternatives[0];
    assert!(!alt.place, "no arrow, so no place marker");

    match &alt.body.kind {
        ExprKind::Seq(parts) => {
            assert!(matches!(&parts[1].kind, ExprKind::Ref(n) if n == "place"));
        }
        other => panic!("expected a sequence, got {other:?}"),
    }
}

#[test]
fn place_after_an_arrow_is_a_marker() {
    let ast = parse_str("grammar A;\nrule r = name:X -> var place;\n").unwrap();
    let alt = &ast.rules[0].alternatives[0];
    assert!(alt.place);
    assert_eq!(alt.label.as_deref().map(String::as_str), Some("var"));
}

/// Rendering is close enough to round-trippable that re-parsing its output
/// must produce identical text. A surprising re-print is a real bug.
#[test]
fn render_round_trips() {
    for path in ["example.nh", "examples/calc.nh", "examples/basic.nh"] {
        let first = render(&ok(&repo(path)));
        let second = render(&parse_str(&first).unwrap_or_else(|e| {
            panic!("re-parsing rendered output of {path} failed:\n{e}\n--- output ---\n{first}")
        }));
        assert_eq!(first, second, "{path} did not round-trip");
    }
}

// ---------------------------------------------------------------------------
// Imports (DESIGN.md §3.1)
// ---------------------------------------------------------------------------

#[test]
fn diamond_import_loads_shared_file_once() {
    let ast = ok(&fixture("diamond.nh"));
    let shared = ast
        .tokens
        .iter()
        .filter(|t| t.name.value == "SHARED_ID")
        .count();
    assert_eq!(shared, 1, "a file reached twice must not duplicate");

    let names: Vec<&str> = ast.tokens.iter().map(|t| t.name.value.as_str()).collect();
    assert!(names.contains(&"LEFT_ONLY") && names.contains(&"RIGHT_ONLY"));
}

#[test]
fn import_cycle_is_an_error() {
    let out = err(&fixture("cycle_a.nh"));
    assert!(out.contains("import cycle detected"), "{out}");
    assert!(out.contains("cycle_b.nh"), "{out}");
}

#[test]
fn duplicate_definition_names_both_locations() {
    let out = err(&fixture("dup_main.nh"));
    assert!(out.contains("token `IDENT` already defined"), "{out}");
    assert!(out.contains("first defined here"), "{out}");
    // Both files must appear: the duplicate and the original.
    assert!(out.contains("dup_main.nh"), "{out}");
    assert!(out.contains("dup_base.nh"), "{out}");
}

#[test]
fn a_fragment_alone_has_no_grammar_declaration() {
    let out = err(&fixture("fragment_only.nh"));
    assert!(out.contains("no `grammar` declaration found"), "{out}");
}

#[test]
fn missing_import_reports_the_import_site() {
    let out = parse_str("grammar A;\nimport \"nope_does_not_exist.nh\";\nrule r = x;\n")
        .unwrap_err();
    assert!(out.contains("cannot read"), "{out}");
    assert!(out.contains("nope_does_not_exist.nh"), "{out}");
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

#[test]
fn parse_errors_carry_file_line_and_column() {
    let out = parse_str("grammar A;\nrule r = ;\n").unwrap_err();
    assert!(out.contains(":2:"), "expected a line 2 location:\n{out}");
    assert!(out.contains("expected"), "{out}");
}
