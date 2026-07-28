//! Tests for `nh build --prune`.
//!
//! The behaviour that matters is what it *refuses* to do. Removing a handler
//! for a rule that no longer exists is helpful; removing one somebody wrote
//! code into is destroying work, and "this rule is gone" is not the same claim
//! as "you do not want this code".

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

const GRAMMAR: &str = r#"
grammar G;
skip WS = " " | "\n";
token ALPHA = @ "a".."z";
token IDENT = @ ALPHA+;
rule top = SOI item* EOI;
rule item = name:IDENT ";" -> named | "?" -> unknown;
"#;

/// The same grammar with both alternatives replaced, orphaning their handlers.
const GRAMMAR_CHANGED: &str = r#"
grammar G;
skip WS = " " | "\n";
token ALPHA = @ "a".."z";
token IDENT = @ ALPHA+;
rule top = SOI item* EOI;
rule item = name:IDENT ";" -> other;
"#;

fn nh() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nh"))
}

struct Project {
    dir: PathBuf,
}

impl Project {
    /// Generates from `GRAMMAR`, then implements one handler and leaves the
    /// other an untouched stub, then swaps in `GRAMMAR_CHANGED`.
    fn with_two_orphans() -> Self {
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join("nh-prune-tests").join(format!(
            "p{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("g.nh"), GRAMMAR).unwrap();

        let p = Project { dir };
        p.build(&[]);

        // One handler gets real code; the other keeps its generated stub.
        std::fs::write(
            p.handler("item_named"),
            "// my real implementation\npub fn run() {}\n",
        )
        .unwrap();

        std::fs::write(p.dir.join("g.nh"), GRAMMAR_CHANGED).unwrap();
        p
    }

    fn handler(&self, name: &str) -> PathBuf {
        self.dir.join("src/handlers").join(format!("{name}.rs"))
    }

    fn build(&self, extra: &[&str]) -> String {
        let src = self.dir.join("src");
        let pest = src.join("g.pest");
        let mut cmd = nh();
        cmd.args([
            "build",
            self.dir.join("g.nh").to_str().unwrap(),
            "-o",
            pest.to_str().unwrap(),
            "--rust",
            src.to_str().unwrap(),
        ]);
        cmd.args(extra);
        let out = cmd.output().expect("running nh build");
        assert!(
            out.status.success(),
            "nh build failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stderr).into_owned()
    }
}

#[test]
fn without_prune_orphans_are_reported_and_kept() {
    let p = Project::with_two_orphans();
    let out = p.build(&[]);

    assert!(out.contains("no longer match any grammar alternative"), "{out}");
    assert!(out.contains("item_named.rs  (implemented"), "{out}");
    assert!(out.contains("item_unknown.rs  (never implemented"), "{out}");
    assert!(out.contains("pass --prune"), "{out}");

    assert!(p.handler("item_named").exists(), "nothing is deleted by default");
    assert!(p.handler("item_unknown").exists());
}

/// `--prune` removes what was never written, and stops at what was.
#[test]
fn prune_removes_untouched_stubs_only() {
    let p = Project::with_two_orphans();
    let out = p.build(&["--prune"]);

    assert!(!p.handler("item_unknown").exists(), "the untouched stub should go");
    assert!(
        p.handler("item_named").exists(),
        "a handler containing code must survive --prune"
    );
    assert!(out.contains("removed handlers/item_unknown.rs"), "{out}");
    assert!(out.contains("pass --prune --force"), "{out}");
}

#[test]
fn force_removes_implemented_orphans_too() {
    let p = Project::with_two_orphans();
    p.build(&["--prune", "--force"]);

    assert!(!p.handler("item_unknown").exists());
    assert!(!p.handler("item_named").exists(), "--force removes it");
}

/// Handlers that still match the grammar are never candidates, implemented or
/// not.
#[test]
fn a_current_handler_is_never_pruned() {
    let p = Project::with_two_orphans();
    p.build(&["--prune", "--force"]);

    // The changed grammar has a single labelled alternative, so the rule
    // collapses and its handler is named after the *rule* — `item`, not
    // `item_other`. Its stub is untouched, so only being current keeps it alive.
    assert!(
        p.handler("item").exists(),
        "an untouched stub for a *current* alternative must survive"
    );
    assert!(p.dir.join("src/handlers/mod.rs").exists(), "mod.rs is generated, not an orphan");
}

#[test]
fn a_clean_project_reports_nothing() {
    let p = Project::with_two_orphans();
    p.build(&["--prune", "--force"]);

    let out = p.build(&["--prune"]);
    assert!(!out.contains("no longer match"), "nothing left to report:\n{out}");
    assert!(!out.contains("removed"), "{out}");
}

/// `--prune` with no `--rust` has nothing to prune, and silently doing nothing
/// would look like it worked.
#[test]
fn prune_without_rust_is_a_usage_error() {
    let p = Project::with_two_orphans();
    let out = nh()
        .args([
            "build",
            p.dir.join("g.nh").to_str().unwrap(),
            "-o",
            p.dir.join("src/g.pest").to_str().unwrap(),
            "--prune",
        ])
        .output()
        .unwrap();

    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("only applies with `--rust"), "{stderr}");
}

/// `nh build -o src/x.pest` in a fresh project should not fail because `src/`
/// does not exist yet — that is the obvious first command.
#[test]
fn build_creates_the_output_directory() {
    let dir = std::env::temp_dir()
        .join("nh-prune-tests")
        .join(format!("mkdir-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("g.nh"), GRAMMAR).unwrap();

    let out = nh()
        .args([
            "build",
            dir.join("g.nh").to_str().unwrap(),
            "-o",
            dir.join("nested/deeper/g.pest").to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(Path::new(&dir.join("nested/deeper/g.pest")).exists());
}
