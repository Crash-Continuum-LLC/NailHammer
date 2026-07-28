//! Tests for `nh init`.
//!
//! The scaffold exists to encode things that fail *silently* when omitted, so
//! it is not enough to check that files appeared. These tests take the
//! generated grammar through the real pipeline and parse the generated sample
//! program with it — if the starter project doesn't work, the feature has no
//! value.

use nh_syntax::{resolve, SourceMap};
use pest_vm::Vm;
use std::path::{Path, PathBuf};
use std::process::Command;

fn nh() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nh"))
}

/// Scaffolds into a fresh temp directory and returns its path.
fn scaffold(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("nh-init-tests")
        .join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let out = nh()
        .args(["init", dir.to_str().unwrap(), "--name", name])
        .output()
        .expect("running nh init");

    assert!(
        out.status.success(),
        "nh init failed:\n{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    dir
}

fn read(dir: &Path, rel: &str) -> String {
    std::fs::read_to_string(dir.join(rel))
        .unwrap_or_else(|e| panic!("missing {rel}: {e}"))
}

#[test]
fn scaffold_creates_a_complete_project() {
    let dir = scaffold("demo");
    for f in [
        "demo.nh",
        "Cargo.toml",
        "README.md",
        ".gitignore",
        "sample.demo",
        "build.rs",
        "src/lib.rs",
        "src/main.rs",
        "src/demo.pest",
        // The Rust side, generated at init so the project builds immediately.
        "src/generated/views.rs",
        "src/generated/dispatch.rs",
        "src/generated/diagnostics.rs",
        "src/generated/place.rs",
        // Working handlers, not `todo!` stubs.
        "src/handlers/program.rs",
        "src/handlers/stmt_bind.rs",
        "src/handlers/primary_num.rs",
    ] {
        assert!(dir.join(f).exists(), "missing {f}");
    }
}

/// A scaffolded project regenerates on `cargo build`, so a grammar edit cannot
/// leave you compiling against stale views.
#[test]
fn the_scaffold_regenerates_on_cargo_build() {
    let dir = scaffold("buildrs");
    let build_rs = read(&dir, "build.rs");

    // It calls the binary rather than linking the generator, which is what
    // keeps a scaffolded project's dependencies down to pest.
    assert!(build_rs.contains("Command::new"), "{build_rs}");
    assert!(build_rs.contains("rerun-if-changed"), "{build_rs}");

    let toml = read(&dir, "Cargo.toml");
    assert!(toml.contains("build = \"build.rs\""), "{toml}");
    assert!(
        !toml.contains("[build-dependencies]"),
        "shelling out means there is nothing to depend on:\n{toml}"
    );
}

/// The runtime travels with the project. Anything fetched over the network is
/// a credential, a cargo setting, or an outage between somebody and a working
/// project — and the runtime was all three before it was vendored.
#[test]
fn the_runtime_is_vendored_into_the_project() {
    let dir = scaffold("vendored");

    assert!(dir.join("vendor/nh-runtime/Cargo.toml").exists());
    for m in ["lib", "ctx", "diagnostic", "error", "name", "node", "ops", "source"] {
        assert!(
            dir.join(format!("vendor/nh-runtime/src/{m}.rs")).exists(),
            "missing vendored module `{m}`"
        );
    }

    let toml = read(&dir, "Cargo.toml");
    assert!(toml.contains(r#"path = "vendor/nh-runtime""#), "{toml}");
    assert!(!toml.contains("git ="), "nothing is fetched:\n{toml}");

    // The vendored manifest must not inherit from a workspace that is not
    // there, and must not be adopted by one that is.
    let vendored = read(&dir, "vendor/nh-runtime/Cargo.toml");
    assert!(!vendored.contains(".workspace = true"), "{vendored}");
    assert!(vendored.contains("[workspace]"), "{vendored}");
}

/// The scaffold ships *working* handlers rather than stubs, so a fresh project
/// does something on the first `cargo run`.
#[test]
fn handlers_are_implemented_not_stubbed() {
    let dir = scaffold("impls");
    let handler = read(&dir, "src/handlers/stmt_bind.rs");
    assert!(
        !handler.contains("is not implemented yet"),
        "the scaffold must ship working handlers:\n{handler}"
    );
    assert!(
        handler.contains("name: &str, value: Value"),
        "and they take their bindings as parameters:\n{handler}"
    );
}

/// The whole point of the scaffold: the generated grammar parses the generated
/// sample program.
#[test]
fn the_generated_grammar_parses_the_generated_sample() {
    let dir = scaffold("runs");

    let mut sm = SourceMap::new();
    let ast = resolve(&mut sm, &dir.join("runs.nh"))
        .unwrap_or_else(|e| panic!("scaffolded grammar did not parse:\n{}", e.render(&sm)));
    let table = nh_operators::resolve(&ast, &mut sm)
        .unwrap_or_else(|e| panic!("operator table failed:\n{}", e.render(&sm)));
    let lowered = nh_lower::lower(&ast, &table)
        .unwrap_or_else(|e| panic!("scaffolded grammar did not lower:\n{}", e.render(&sm)));

    // This validation path includes `validate_tag_silent_rules`, which is what
    // `pest_derive` runs. Without the `grammar-extras` feature on `pest_meta`
    // it would be compiled out and this test would pass a broken grammar.
    let rules = match pest_meta::parse_and_optimize(&lowered.pest) {
        Ok((_, rules)) => rules,
        Err(errors) => panic!(
            "scaffolded grammar is not valid pest:\n{}\n--- grammar ---\n{}",
            errors
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("\n"),
            lowered.pest
        ),
    };

    let vm = Vm::new(rules);
    let sample = read(&dir, "sample.runs");
    let pairs = vm
        .parse("program", &sample)
        .unwrap_or_else(|e| panic!("sample program did not parse:\n{e}"));

    let produced: Vec<String> = pairs.flatten().map(|p| p.as_rule().to_string()).collect();
    for expected in ["stmt_bind", "stmt_print", "primary_num", "primary_var"] {
        assert!(produced.contains(&expected.to_string()), "{produced:?}");
    }
}

/// The `.pest` written at init time must match what `nh build` would produce,
/// or the starter is stale the moment it is created.
#[test]
fn the_committed_pest_matches_a_fresh_build() {
    let dir = scaffold("fresh");

    let mut sm = SourceMap::new();
    let ast = resolve(&mut sm, &dir.join("fresh.nh")).unwrap();
    let table = nh_operators::resolve(&ast, &mut sm).unwrap();
    let lowered = nh_lower::lower(&ast, &table).unwrap();

    assert_eq!(read(&dir, "src/fresh.pest"), lowered.pest);
}

// ---------------------------------------------------------------------------
// The silent footguns the scaffold exists to prevent
// ---------------------------------------------------------------------------

#[test]
fn cargo_toml_enables_grammar_extras() {
    let dir = scaffold("extras");
    let toml = read(&dir, "Cargo.toml");
    assert!(
        toml.contains("grammar-extras"),
        "without this feature every node tag is silently ignored:\n{toml}"
    );
}

#[test]
fn the_entry_rule_is_anchored() {
    let dir = scaffold("anchor");
    let grammar = read(&dir, "anchor.nh");
    assert!(
        grammar.contains("SOI") && grammar.contains("EOI"),
        "an unanchored entry rule fails on a leading blank line:\n{grammar}"
    );
}

/// A program starting with a comment and a blank line is the exact case an
/// unanchored grammar mishandles. The sample deliberately starts with one.
#[test]
fn a_program_starting_with_trivia_parses() {
    let dir = scaffold("trivia");
    let sample = read(&dir, "sample.trivia");
    assert!(
        sample.starts_with("//"),
        "the sample should start with a comment, to exercise anchoring"
    );

    let mut sm = SourceMap::new();
    let ast = resolve(&mut sm, &dir.join("trivia.nh")).unwrap();
    let table = nh_operators::resolve(&ast, &mut sm).unwrap();
    let lowered = nh_lower::lower(&ast, &table).unwrap();
    let (_, rules) = pest_meta::parse_and_optimize(&lowered.pest).unwrap();

    Vm::new(rules)
        .parse("program", &format!("\n\n{sample}"))
        .unwrap_or_else(|e| panic!("leading trivia must parse:\n{e}"));
}

// ---------------------------------------------------------------------------
// Behaviour
// ---------------------------------------------------------------------------

#[test]
fn init_refuses_a_non_empty_directory() {
    let dir = scaffold("occupied");

    let out = nh()
        .args(["init", dir.to_str().unwrap(), "--name", "occupied"])
        .output()
        .unwrap();

    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("not empty"), "{stderr}");
    assert!(stderr.contains("--force"), "{stderr}");
}

#[test]
fn force_overwrites_a_non_empty_directory() {
    let dir = scaffold("forced");
    let out = nh()
        .args([
            "init",
            dir.to_str().unwrap(),
            "--name",
            "forced",
            "--force",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn custom_extension_is_honoured() {
    let dir = std::env::temp_dir()
        .join("nh-init-tests")
        .join(format!("ext-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let out = nh()
        .args([
            "init",
            dir.to_str().unwrap(),
            "--name",
            "toy",
            "--ext",
            "ty",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(dir.join("sample.ty").exists(), "sample should use --ext");
    assert!(read(&dir, "src/main.rs").contains("sample.ty"));
}

/// The full end-to-end check: the scaffolded project actually compiles and
/// runs. Ignored by default because it builds pest from scratch; the fast tests
/// above use the same validation path `pest_derive` does.
///
///     cargo test -p nh-cli -- --ignored
#[test]
#[ignore = "compiles a whole cargo project"]
fn scaffolded_project_builds_and_runs() {
    let dir = scaffold("e2e");
    let out = Command::new(env!("CARGO"))
        .current_dir(&dir)
        .args(["run", "--quiet"])
        .output()
        .expect("running cargo in the scaffolded project");

    assert!(
        out.status.success(),
        "scaffolded project failed to build:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // The sample program's actual output. This exercises the whole stack:
    // generated views, handler dispatch, and the operator driver's precedence.
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines,
        vec!["28", "22", "5", "14"],
        "scaffolded interpreter produced the wrong answers:\n{stdout}"
    );
}

/// DESIGN.md §5.4: adding an alternative to the grammar breaks the build until
/// a handler exists.
///
/// This only holds because the generated stub is a `compile_error!`. A stub
/// that compiled and failed at run time would let an unhandled alternative ship.
#[test]
#[ignore = "compiles a whole cargo project"]
fn an_unhandled_alternative_breaks_the_build() {
    let dir = scaffold("unhandled");

    let grammar = dir.join("unhandled.nh");
    let text = std::fs::read_to_string(&grammar).unwrap().replace(
        "  | value:expr \";\"                      -> eval",
        "  | \"show\" value:expr \";\"               -> show\n  | value:expr \";\"                      -> eval",
    );
    std::fs::write(&grammar, text).unwrap();

    let out = Command::new(env!("CARGO"))
        .current_dir(&dir)
        .args(["build", "--quiet"])
        .output()
        .expect("running cargo");

    assert!(!out.status.success(), "an unhandled alternative must fail the build");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("handler `stmt_show` is not implemented"),
        "the error should name the handler and point at its file:\n{stderr}"
    );
    assert!(
        dir.join("src/handlers/stmt_show.rs").exists(),
        "and the stub should be waiting"
    );
}

/// Recovery, in a scaffolded project: one error reported, every good statement
/// still evaluated.
#[test]
#[ignore = "compiles a whole cargo project"]
fn a_scaffolded_project_recovers_from_syntax_errors() {
    let dir = scaffold("recov");
    std::fs::write(
        dir.join("broken.recov"),
        "print 1;\n@@@ ;\nprint 2;\nlet x = 3;\nprint x * 10;\n",
    )
    .unwrap();

    let out = Command::new(env!("CARGO"))
        .current_dir(&dir)
        .args(["run", "--quiet", "--", "broken.recov"])
        .output()
        .expect("running the scaffolded project");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(stderr.contains("could not parse this `stmt`"), "{stderr}");
    assert_eq!(
        stderr.matches("could not parse").count(),
        1,
        "one error, not a cascade:\n{stderr}"
    );
    assert_eq!(
        stdout.lines().collect::<Vec<_>>(),
        vec!["1", "2", "30"],
        "every statement that could run should have run:\n{stdout}"
    );
    assert!(!out.status.success(), "a recovered error is still a failure");
}
