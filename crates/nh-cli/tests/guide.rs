//! The guide's grammars are checked, not just written.
//!
//! `guide/` is a step-by-step book, so every grammar in it is one a reader will
//! paste. Documentation drifts silently; this makes it fail the build instead.
//!
//! Only *complete* grammars are checked — a chapter that shows two rules in
//! isolation, or a header with no rules yet, is illustrating a shape rather
//! than offering a file. A block counts as complete when it declares both a
//! `grammar` and at least one `rule`.

use std::path::{Path, PathBuf};
use std::process::Command;

fn nh() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nh"))
}

fn guide_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("guide")
}

/// Every ```nh block that declares a grammar, with the file and line it came
/// from so a failure names the place to fix.
fn complete_grammars() -> Vec<(String, usize, String)> {
    let mut out = Vec::new();
    let mut files: Vec<PathBuf> = std::fs::read_dir(guide_dir())
        .expect("a guide/ directory")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "md"))
        .collect();
    files.sort();

    for path in files {
        let text = std::fs::read_to_string(&path).unwrap();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let mut line_no = 0;
        let mut lines = text.lines();
        while let Some(line) = lines.next() {
            line_no += 1;
            if line.trim() != "```nh" {
                continue;
            }
            let start = line_no + 1;
            let mut body = String::new();
            for l in lines.by_ref() {
                line_no += 1;
                if l.trim() == "```" {
                    break;
                }
                body.push_str(l);
                body.push('\n');
            }
            let has_grammar = body
                .lines()
                .any(|l| l.trim_start().starts_with("grammar "));
            let has_rule = body.lines().any(|l| l.trim_start().starts_with("rule "));
            if has_grammar && has_rule {
                out.push((name.clone(), start, body));
            }
        }
    }
    out
}

#[test]
fn every_complete_grammar_in_the_guide_checks() {
    let grammars = complete_grammars();
    assert!(
        !grammars.is_empty(),
        "found no complete grammars in guide/ — the extractor is broken, \
         which would make this test pass for the wrong reason"
    );

    let dir = std::env::temp_dir()
        .join("nh-guide-tests")
        .join(std::process::id().to_string());
    std::fs::create_dir_all(&dir).unwrap();

    let mut failures = Vec::new();
    for (file, line, body) in &grammars {
        let path = dir.join(format!("{}-{line}.nh", file.replace('.', "_")));
        std::fs::write(&path, body).unwrap();
        let out = nh()
            .args(["check", path.to_str().unwrap()])
            .output()
            .expect("running nh check");
        if !out.status.success() {
            failures.push(format!(
                "guide/{file}:{line}\n{}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} guide grammars no longer check:\n\n{}",
        failures.len(),
        grammars.len(),
        failures.join("\n")
    );
}
