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
    scaffold_with(name, &[])
}

fn scaffold_with(name: &str, extra: &[&str]) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("nh-init-tests")
        .join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let out = nh()
        .args(["init", dir.to_str().unwrap(), "--name", name])
        .args(extra)
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
        .env("NH", env!("CARGO_BIN_EXE_nh"))
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
        vec!["28", "22", "5", "14", "1"],
        "scaffolded interpreter produced the wrong answers:\n{stdout}"
    );
}

/// `--compiler` scaffolds the other shape, and it produces the **same answers**.
///
/// That is the whole claim: one grammar, one set of handler signatures, and the
/// only difference is `type Out = ()` plus bodies that emit instead of compute.
/// If these two ever disagree, something has become interpreter-shaped that
/// should not be.
#[test]
#[ignore = "compiles a whole cargo project"]
fn the_compiler_scaffold_gives_the_same_answers_as_the_interpreter() {
    let dir = scaffold_with("e2ec", &["--compiler"]);
    let out = Command::new(env!("CARGO"))
        .current_dir(&dir)
        .env("NH", env!("CARGO_BIN_EXE_nh"))
        .args(["run", "--quiet"])
        .output()
        .expect("running cargo in the scaffolded compiler");

    assert!(
        out.status.success(),
        "scaffolded compiler failed to build:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines,
        vec!["28", "22", "5", "14", "1"],
        "the compiled program must compute what the interpreted one did:\n{stdout}"
    );

    // And it really did compile rather than interpret.
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--- bytecode ---") && stderr.contains("Mul"),
        "no instruction stream in sight:\n{stderr}"
    );
}

/// A compiler scaffold implements no `Values`, and says so where it must.
#[test]
fn the_compiler_scaffold_asks_no_questions_about_values() {
    let dir = scaffold_with("shape", &["--compiler"]);
    let lib = std::fs::read_to_string(dir.join("src/lib.rs")).unwrap();

    assert!(lib.contains("type Out = ();"), "{lib}");
    assert!(
        !lib.contains("impl generated::dispatch::Values"),
        "a compiler has no values to inspect:\n{lib}"
    );
    assert!(
        lib.contains("nh_handlers!(Interp, without short_circuit)"),
        "it must opt out, having no `truthy` for the generated impl:\n{lib}"
    );
    assert!(
        lib.contains("impl generated::dispatch::ShortCircuit for Interp"),
        "and then supply its own:\n{lib}"
    );
}

/// Both shapes share `main.rs` bar one arm — including the whole error path,
/// which is the boilerplate the scaffold exists to provide.
#[test]
fn both_shapes_get_the_same_error_handling() {
    for extra in [&[][..], &["--compiler"][..]] {
        let dir = scaffold_with(if extra.is_empty() { "mi" } else { "mc" }, extra);
        let main = std::fs::read_to_string(dir.join("src/main.rs")).unwrap();

        assert!(
            main.contains("generated::eval_source(&mut interp, &mut cx, file)"),
            "{extra:?} must use the generated driver:\n{main}"
        );
        assert!(
            main.contains("d.render(cx.sources())"),
            "{extra:?} must scaffold the formatting loop:\n{main}"
        );
        // Parsing, syntax-error collection and tree building are the driver's.
        for gone in ["::parse(Rule::", "syntax_errors", "build_program", "cx.enter"] {
            assert!(
                !main.contains(gone),
                "{extra:?} still hand-writes `{gone}`:\n{main}"
            );
        }
    }
}

/// Every style × feature combination scaffolds, builds, and **the two shapes
/// agree**.
///
/// This is the claim the whole picker rests on: one grammar description drives
/// an interpreter and a compiler, and picking a syntax or a feature set does
/// not quietly favour one of them. If these ever disagree, something has become
/// interpreter-shaped that should not be.
#[test]
#[ignore = "compiles sixteen cargo projects"]
fn every_combination_runs_and_the_two_shapes_agree() {
    for style in ["c", "basic"] {
        for with in ["none", "loops", "functions", "all"] {
            let interp = scaffold_with(
                &format!("mx{style}{with}"),
                &["--style", style, "--with", with],
            );
            let compiler = scaffold_with(
                &format!("mx{style}{with}c"),
                &["--style", style, "--with", with, "--compiler"],
            );

            let a = run_scaffold(&interp);
            let b = run_scaffold(&compiler);
            assert_eq!(
                a, b,
                "{style}/{with}: the compiled program must compute what the \
                 interpreted one did"
            );
            assert!(!a.is_empty(), "{style}/{with} produced nothing");
        }
    }
}

/// The sample program's output, or a panic naming what went wrong.
fn run_scaffold(dir: &Path) -> Vec<String> {
    let out = Command::new(env!("CARGO"))
        .current_dir(dir)
        .env("NH", env!("CARGO_BIN_EXE_nh"))
        .args(["run", "--quiet"])
        .output()
        .expect("running cargo in the scaffolded project");

    assert!(
        out.status.success(),
        "{} failed:\n{}",
        dir.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

/// Both styles share one set of handler files.
///
/// `WHILE cond .. WEND` and `while cond { }` bind the same names to the same
/// shapes, so `stmt_while.rs` does not know which syntax reached it. That is
/// what binding by name instead of position buys, and it is why the picker
/// needs one handler per feature rather than two.
///
/// **One thing does differ, and it is a language difference rather than a
/// syntax one.** The line-oriented style folds identifier case, as BASIC always
/// has, so a name arrives as `&Name` — carrying both the folded key and the
/// spelling as written — where the C style gets a plain `&str`. Everything else
/// about the signature, and the whole body, is the same file.
#[test]
fn both_styles_share_their_handlers() {
    let c = scaffold_with("shc", &["--style", "c", "--with", "all"]);
    let b = scaffold_with("shb", &["--style", "basic", "--with", "all"]);

    let names = |dir: &Path| -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir.join("src/handlers"))
            .expect("a handlers directory")
            .map(|e| e.expect("an entry").file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    };

    let (mut cn, bn) = (names(&c), names(&b));
    // The line-oriented style needs one extra: a `;` terminates a statement on
    // its own, a newline needs a rule to hang on.
    assert!(bn.contains(&"line.rs".to_string()), "{bn:?}");
    cn.push("line.rs".to_string());
    cn.sort();
    assert_eq!(cn, bn, "the two styles must need the same handlers");

    // `mod.rs` is the generated module list, not a handler.
    let shared: Vec<&String> = bn.iter().filter(|n| *n != "line.rs" && *n != "mod.rs").collect();
    assert!(shared.len() > 10, "expected a real handler set, got {shared:?}");

    let mut folded = 0;
    for name in shared {
        let read = |dir: &Path| {
            std::fs::read_to_string(dir.join("src/handlers").join(name)).expect("a handler")
        };
        let (cs, bs) = (read(&c), read(&b));

        // Normalising the identifier type is the *only* licence taken here. If
        // anything else drifts apart, this fails.
        let normalise = |s: &str| {
            s.replace("&Name", "&str")
                .replace("use nh_runtime::Name;\n", "")
                .replace(".key()", "")
        };
        assert_eq!(
            normalise(&cs),
            normalise(&bs),
            "{name} differs between styles by more than how a name is spelled"
        );

        if bs.contains("&Name") {
            folded += 1;
            assert!(
                cs.contains("&str"),
                "{name}: the C style does not fold, so it should take `&str`"
            );
        }
    }

    assert_eq!(
        folded, 7,
        "seven handlers touch a name; if this moves, the folding decision \
         reached somewhere new and is worth a second look"
    );
}


/// The line-oriented style folds identifier case, as BASIC always has.
///
/// This costs the scaffold something real — a folding token binds as `&Name`
/// rather than `&str`, so the two styles no longer produce byte-identical
/// handlers — and it is worth it. A BASIC where `Total` and `total` are
/// different variables is not a BASIC.
#[test]
#[ignore = "compiles two cargo projects"]
fn the_line_oriented_style_folds_identifier_case() {
    const PROGRAM: &str = "\
LET Total = 5
PRINT total
LET counter = 2
PRINT COUNTER * Total

FUNCTION Double(N)
  RETURN n * 2
END FUNCTION
PRINT double(21)

FOR Idx = 1 TO 3
  PRINT idx
NEXT
";

    for (label, extra) in [
        ("interpreter", &["--style", "basic", "--with", "all"][..]),
        (
            "compiler",
            &["--style", "basic", "--with", "all", "--compiler"][..],
        ),
    ] {
        let dir = scaffold_with(&format!("fold{}", &label[..4]), extra);
        std::fs::write(dir.join("prog.txt"), PROGRAM).unwrap();

        let out = Command::new(env!("CARGO"))
            .current_dir(&dir)
            .env("NH", env!("CARGO_BIN_EXE_nh"))
            .args(["run", "--quiet", "--", "prog.txt"])
            .output()
            .expect("running cargo");

        let stdout = String::from_utf8_lossy(&out.stdout);
        let lines: Vec<&str> = stdout.lines().collect();
        assert_eq!(
            lines,
            // variables, then a call whose *name* and *parameter* both fold,
            // then a loop variable.
            vec!["5", "10", "42", "1", "2", "3"],
            "{label}: case should not matter here:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

/// Folding must not reach the *diagnostics*. `Name` keeps both spellings for
/// exactly this: reporting `missing` when the programmer typed `Missing` reads
/// as a bug in their language rather than in their program.
#[test]
#[ignore = "compiles a cargo project"]
fn an_error_reports_the_spelling_that_was_typed() {
    let dir = scaffold_with("foldmsg", &["--style", "basic"]);
    std::fs::write(dir.join("prog.txt"), "PRINT Missing\n").unwrap();

    let out = Command::new(env!("CARGO"))
        .current_dir(&dir)
        .env("NH", env!("CARGO_BIN_EXE_nh"))
        .args(["run", "--quiet", "--", "prog.txt"])
        .output()
        .expect("running cargo");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("`Missing`"),
        "the diagnostic should echo what was written:\n{stderr}"
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
        "  | value:expr \";\"                                           -> eval",
        "  | \"show\" value:expr \";\"                                    -> show\n           | value:expr \";\"                                           -> eval",
    );
    std::fs::write(&grammar, text).unwrap();

    let out = Command::new(env!("CARGO"))
        .current_dir(&dir)
        .env("NH", env!("CARGO_BIN_EXE_nh"))
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
        .env("NH", env!("CARGO_BIN_EXE_nh"))
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

/// A scaffolded project must build for somebody who does not have `nh`.
///
/// The generated code is committed, so it compiles on its own; requiring the
/// tool to *build* would mean a project could not be handed to a colleague or
/// built in CI without installing the generator first. That is exactly what
/// broke when `build.rs` stopped depending on `nh-build` and started shelling
/// out — every scaffolded project failed on a machine without `nh`.
#[test]
#[ignore = "compiles a scaffolded project"]
fn a_scaffolded_project_builds_without_nh_installed() {
    let dir = scaffold("nonh");

    let out = Command::new(env!("CARGO"))
        .current_dir(&dir)
        // A name nothing will resolve, standing in for "not installed".
        .env("NH", "nh-that-is-not-installed")
        .args(["run", "--quiet"])
        .output()
        .expect("running cargo");

    assert!(
        out.status.success(),
        "a project must build without the tool:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).lines().collect::<Vec<_>>(),
        vec!["28", "22", "5", "14", "1"],
    );
    // `--quiet` suppresses `cargo:warning`, so the notice is checked with a
    // separate ordinary build rather than by loosening the run above.
    let noisy = Command::new(env!("CARGO"))
        .current_dir(&dir)
        .env("NH", "nh-that-is-not-installed")
        .args(["build"])
        .output()
        .expect("running cargo");
    let stderr = String::from_utf8_lossy(&noisy.stderr);
    assert!(
        stderr.contains("not found") && stderr.contains("will not take effect"),
        "it should say the grammar will not be regenerated:\n{stderr}"
    );
}

/// ...but an edited grammar with no tool must stop, rather than quietly
/// compiling the previous one.
#[test]
#[ignore = "compiles a scaffolded project"]
fn an_edited_grammar_without_nh_is_a_build_error() {
    let dir = scaffold("stale");

    // Build once so the generated output exists and is current.
    let first = Command::new(env!("CARGO"))
        .current_dir(&dir)
        .env("NH", env!("CARGO_BIN_EXE_nh"))
        .args(["build", "--quiet"])
        .output()
        .expect("running cargo");
    assert!(first.status.success(), "{}", String::from_utf8_lossy(&first.stderr));

    // Touch the grammar so it is newer than what was generated from it.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let grammar = dir.join("stale.nh");
    let text = std::fs::read_to_string(&grammar).unwrap();
    std::fs::write(&grammar, format!("{text}\n// edited\n")).unwrap();

    let out = Command::new(env!("CARGO"))
        .current_dir(&dir)
        .env("NH", "nh-that-is-not-installed")
        .args(["build", "--quiet"])
        .output()
        .expect("running cargo");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "stale output must not compile:\n{stderr}");
    assert!(stderr.contains("has changed"), "{stderr}");
    assert!(stderr.contains("previous grammar"), "{stderr}");
}

/// `--help` is the first thing a new user reads, and it had been claiming the
/// operator driver was unimplemented long after M3 shipped. Nothing failed
/// when that went stale, which is exactly why it needs a test.
#[test]
fn help_lists_every_flag_and_claims_nothing_untrue() {
    let out = Command::new(env!("CARGO_BIN_EXE_nh"))
        .arg("--help")
        .output()
        .expect("running nh --help");
    let help = String::from_utf8_lossy(&out.stdout);

    for flag in [
        "--json",
        "--deny-warnings",
        "--prune",
        "--lints",
        "--source",
        "--ext",
        "--style",
        "--with",
        "--compiler",
        "--async",
    ] {
        assert!(help.contains(flag), "`{flag}` is undocumented:\n{help}");
    }
    assert!(
        !help.contains("Not yet implemented"),
        "every milestone is complete; the help should not say otherwise:\n{help}"
    );
}

/// `--async` sets a project up for async work in handlers, and a project
/// without it must not pay for tokio at all.
#[test]
fn async_scaffolds_differ_only_where_they_should() {
    let plain = scaffold("plainproj");
    for f in ["Cargo.toml", "src/main.rs", "src/lib.rs"] {
        assert!(
            !read(&plain, f).contains("tokio"),
            "a plain scaffold must not mention tokio in {f}"
        );
    }

    let dir = scaffold_with("asyncproj", &["--async"]);

    // `rt-multi-thread` is not optional: `block_in_place` panics on the
    // current-thread runtime, so the feature and the flavor must agree.
    let toml = read(&dir, "Cargo.toml");
    assert!(toml.contains("tokio"), "{toml}");
    assert!(toml.contains("rt-multi-thread"), "{toml}");

    let main = read(&dir, "src/main.rs");
    assert!(main.contains(r#"#[tokio::main(flavor = "multi_thread")]"#), "{main}");
    assert!(main.contains("async fn main"), "{main}");

    // The helper exists, and uses `block_in_place` rather than the spelling
    // that panics inside a runtime.
    let lib = read(&dir, "src/lib.rs");
    assert!(lib.contains("pub fn block_on"), "{lib}");
    assert!(
        lib.contains("block_in_place"),
        "`Handle::block_on` alone panics inside a runtime:\n{lib}"
    );
}

/// ...and the async scaffold has to actually build and run.
#[test]
#[ignore = "compiles a scaffolded project"]
fn an_async_scaffold_builds_and_runs() {
    let dir = scaffold_with("asyncbuild", &["--async"]);
    let out = Command::new(env!("CARGO"))
        .current_dir(&dir)
        .env("NH", env!("CARGO_BIN_EXE_nh"))
        .args(["run", "--quiet"])
        .output()
        .expect("running cargo");

    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).lines().collect::<Vec<_>>(),
        vec!["28", "22", "5", "14", "1"],
    );
}
