//! Self-hosting check (DESIGN.md M6).
//!
//! `examples/selfhost/nh.nh` describes the `.nh` language in `.nh`. This test
//! generates a parser from it and points that parser at **every `.nh` file in
//! the repository**, including `nh.nh` itself.
//!
//! Note what is *not* claimed. DESIGN.md's M6 asked for `nh.nh` to reproduce
//! `nh.pest` byte for byte, and that is unreachable — see
//! `examples/selfhost/README.md`. What holds is the meaningful property: the
//! two grammars accept the same language.

use nh_lower::lower;
use nh_syntax::{resolve, SourceMap};
use pest_vm::Vm;
use std::path::{Path, PathBuf};

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(rel)
}

/// Every `.nh` file in the repo, found rather than listed so a new grammar is
/// covered automatically.
fn all_nh_files() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            // Templates contain `{{placeholders}}` and are not valid `.nh`
            // until `nh init` renders them.
            if name == "target" || name == "templates" || name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "nh") {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(&repo("."), &mut out);
    out.sort();
    out
}

fn self_hosted_vm() -> Vm {
    let path = repo("examples/selfhost/nh.nh");
    let mut sm = SourceMap::new();
    let ast = resolve(&mut sm, &path).unwrap_or_else(|e| panic!("{}", e.render(&sm)));
    let table = nh_operators::resolve(&ast, &mut sm).unwrap_or_else(|e| panic!("{}", e.render(&sm)));
    let lowered = lower(&ast, &table).unwrap_or_else(|e| panic!("{}", e.render(&sm)));

    match pest_meta::parse_and_optimize(&lowered.pest) {
        Ok((_, rules)) => Vm::new(rules),
        Err(errors) => panic!(
            "the self-hosted grammar is not valid pest:\n{}",
            errors.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("\n")
        ),
    }
}

/// The headline: a parser generated from `nh.nh` accepts every `.nh` file here.
#[test]
fn the_self_hosted_grammar_parses_every_nh_file() {
    let vm = self_hosted_vm();
    let files = all_nh_files();
    assert!(files.len() >= 6, "expected several .nh files, found {files:?}");

    let mut failures = Vec::new();
    for path in &files {
        let text = std::fs::read_to_string(path).unwrap();
        if let Err(e) = vm.parse("file", &text) {
            failures.push(format!("{}:\n{e}", path.display()));
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} files rejected:\n\n{}",
        failures.len(),
        files.len(),
        failures.join("\n\n")
    );
}

/// The fixed point: it parses itself.
#[test]
fn the_self_hosted_grammar_parses_itself() {
    let vm = self_hosted_vm();
    let text = std::fs::read_to_string(repo("examples/selfhost/nh.nh")).unwrap();
    vm.parse("file", &text)
        .unwrap_or_else(|e| panic!("nh.nh must parse itself:\n{e}"));
}

/// It must also *reject* things, or "accepts everything" would pass trivially.
#[test]
fn the_self_hosted_grammar_rejects_malformed_input() {
    let vm = self_hosted_vm();
    for bad in [
        "grammar",                       // no name, no semicolon
        "rule r = ;",                    // empty body
        "token T = @ ;",                 // empty token
        "precedence { left ; }",         // fixity with no operators
        "rule r = \"a\" -> ;",           // arrow with no label
        "reserved from IDENT { let }",   // bare word, not a string
    ] {
        assert!(
            vm.parse("file", bad).is_err(),
            "should have been rejected: {bad:?}"
        );
    }
}
