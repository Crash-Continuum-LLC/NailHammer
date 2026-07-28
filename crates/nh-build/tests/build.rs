//! Tests for `build.rs` integration.
//!
//! The properties that matter are the ones that make it safe to run on *every*
//! build: it must not clobber hand-written handlers, and it must not touch
//! unchanged files (which would make cargo rebuild everything, every time).

use nh_build::Builder;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

const GRAMMAR: &str = r#"
grammar T;
use operators::core;
skip WS = " " | "\n";
token DIGIT = @ "0".."9";
token ALPHA = @ "a".."z";
token NUMBER = @ DIGIT+;
token IDENT = @ ALPHA+;
rule program = SOI stmts:stmt* EOI -> doc;
rule stmt = value:expr ";" -> eval;
rule atom = primary;
rule primary = digits:NUMBER -> num | name:IDENT -> var;
"#;

/// A fresh temp project containing just the grammar.
fn project() -> PathBuf {
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join("nh-build-tests").join(format!(
        "p{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("t.nh"), GRAMMAR).unwrap();
    dir
}

fn build_in(dir: &PathBuf) -> nh_build::Generated {
    // `root` rather than `set_current_dir`: the working directory is process
    // state, and these tests run in parallel.
    Builder::new("t.nh")
        .root(dir)
        .try_run()
        .unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn it_generates_the_pest_and_the_rust_side() {
    let dir = project();
    build_in(&dir);

    for f in [
        "src/t.pest",
        "src/generated/views.rs",
        "src/generated/dispatch.rs",
        "src/handlers/stmt.rs",
    ] {
        assert!(dir.join(f).exists(), "missing {f}");
    }
}

/// The property that makes it safe on every build: an unchanged file is not
/// rewritten, so its mtime does not move and cargo does not rebuild the world.
#[test]
fn a_second_build_writes_nothing() {
    let dir = project();
    let first = build_in(&dir);
    assert!(!first.written.is_empty(), "the first build writes files");

    let second = build_in(&dir);
    assert!(
        second.written.is_empty(),
        "an unchanged grammar must rewrite nothing, or cargo rebuilds \
         everything on every build: {:?}",
        second.written
    );
    assert!(second.created.is_empty(), "no new stubs either");
    assert!(!second.kept.is_empty(), "existing handlers are kept");
}

/// A build script that clobbered hand-written handlers would be unusable.
#[test]
fn hand_written_handlers_are_never_overwritten() {
    let dir = project();
    build_in(&dir);

    let handler = dir.join("src/handlers/stmt.rs");
    std::fs::write(&handler, "// mine\n").unwrap();

    let out = build_in(&dir);
    assert_eq!(std::fs::read_to_string(&handler).unwrap(), "// mine\n");
    assert!(out.kept.contains(&handler), "should be reported as kept");
}

#[test]
fn a_changed_grammar_regenerates_and_stubs_the_new_alternative() {
    let dir = project();
    build_in(&dir);

    let grammar = dir.join("t.nh");
    let updated = std::fs::read_to_string(&grammar)
        .unwrap()
        .replace("| name:IDENT -> var;", "| name:IDENT -> var | \"?\" -> huh;");
    std::fs::write(&grammar, updated).unwrap();

    let out = build_in(&dir);
    assert!(!out.written.is_empty(), "the grammar changed");
    let stub = dir.join("src/handlers/primary_huh.rs");
    assert!(out.created.contains(&stub), "a stub for the new alternative");

    // DESIGN.md §5.4: the build must fail until a handler exists.
    let text = std::fs::read_to_string(&stub).unwrap();
    assert!(
        text.contains("compile_error!"),
        "an unimplemented handler must break the build, not fail at run time:\n{text}"
    );
}

#[test]
fn a_broken_grammar_reports_instead_of_writing() {
    let dir = project();
    std::fs::write(dir.join("t.nh"), "grammar T;\nrule r = missing_thing;\n").unwrap();

    let err = Builder::new("t.nh")
        .root(&dir)
        .try_run()
        .expect_err("a broken grammar must fail the build");
    assert!(err.contains("undefined reference `missing_thing`"), "{err}");
    assert!(!dir.join("src/t.pest").exists(), "nothing should be written");
}
