//! `nh build --target` (VM-DESIGN.md §7.2, §8.3).
//!
//! Two behaviours matter and they are opposites: when the machine can execute
//! every role the grammar binds, the operator implementation is *generated* and
//! the author writes none of it. When it cannot, the build *stops and says
//! which*, rather than emitting code that will not compile.

use std::path::PathBuf;
use std::process::Command;

fn nh() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nh"))
}

/// A grammar whose operators nh-vm can all execute.
const SUPPORTED: &str = r#"grammar Lang;
use operators::none;
skip WS = " " | "\t" | "\n";
token DIGIT = @ "0".."9";
token NUMBER = @ DIGIT+;
token ALPHA = @ "a".."z";
token IDENT = @ ALPHA+;
precedence {
    left  "<" | ">" -> compare;
    left  "+" | "-";
    left  "*" | "/";
    prefix "-";
    atom atom;
}
rule program = SOI stmt+ EOI;
rule stmt = "print" value:expr ";" -> print;
rule atom = primary;
rule primary = value:NUMBER -> num | name:IDENT -> var place;
"#;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("nh-target-tests")
        .join(format!("{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

fn write(dir: &std::path::Path, source: &str) -> PathBuf {
    let g = dir.join("lang.nh");
    std::fs::write(&g, source).unwrap();
    g
}

#[test]
fn a_supported_grammar_generates_its_operator_implementation() {
    let dir = scratch("ok");
    let g = write(&dir, SUPPORTED);

    let out = nh()
        .args(["build"])
        .arg(&g)
        .arg("-o")
        .arg(dir.join("src/lang.pest"))
        .arg("--rust")
        .arg(dir.join("src"))
        .args(["--target", "nh-vm"])
        .output()
        .expect("running nh build");

    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));

    let generated = std::fs::read_to_string(dir.join("src/generated/vm_operators.rs"))
        .expect("vm_operators.rs must exist");

    // A module, not an orphan to `include!`: it brings its own imports and the
    // generator wired it into mod.rs, so the author writes no glue at all.
    assert!(generated.contains("use nh_vm::{Cmp, Emitter, Op, Reg};"), "the Emitter trait must be in scope: {generated}");
    assert!(generated.contains("use super::dispatch::{CompareOp, Operators};"), "{generated}");
    // Imports track what the body uses: this grammar has no lazy operator, so
    // nothing here mentions `ShortCircuit` or `Shared`. An unused import in
    // generated code is a warning the author cannot act on.
    let imports: String = generated
        .lines()
        .filter(|l| l.starts_with("use "))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!imports.contains("ShortCircuit"), "{imports}");
    assert!(!imports.contains("Shared"), "{imports}");
    let modrs = std::fs::read_to_string(dir.join("src/generated/mod.rs")).unwrap();
    assert!(modrs.contains("pub mod vm_operators;"), "{modrs}");

    assert!(generated.contains("fn add(&mut self, lhs: Reg, rhs: Reg)"), "{generated}");
    assert!(generated.contains("self.emit(Op::Add { dst, a: lhs, b: rhs });"), "{generated}");
    assert!(generated.contains("fn compare(&mut self, lhs: Reg, op: CompareOp"), "{generated}");
    assert!(generated.contains("fn neg(&mut self, operand: Reg)"), "{generated}");

    // Nothing is left for a person to fill in — that is the claim.
    assert!(!generated.contains("todo!"), "{generated}");
    assert!(!generated.contains("unsupported"), "{generated}");
}

/// Without `--target`, nothing changes: the file is not generated and the
/// author gets the trait to implement, exactly as before.
#[test]
fn without_a_target_nothing_is_generated() {
    let dir = scratch("notarget");
    let g = write(&dir, SUPPORTED);

    let out = nh()
        .args(["build"])
        .arg(&g)
        .arg("-o")
        .arg(dir.join("src/lang.pest"))
        .arg("--rust")
        .arg(dir.join("src"))
        .output()
        .expect("running nh build");

    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(
        !dir.join("src/generated/vm_operators.rs").exists(),
        "--target is opt-in"
    );
}

/// §8.3 — assistance, not enforcement. The build stops, names every role the
/// machine cannot execute, and names a spelling the author actually typed.
#[test]
fn roles_the_target_cannot_execute_stop_the_build() {
    let dir = scratch("unsupported");
    // `c_style` binds far more than this prototype VM implements.
    let g = write(
        &dir,
        "grammar Lang;\nuse operators::c_style;\n\
         skip WS = \" \" | \"\\t\" | \"\\n\";\n\
         token DIGIT = @ \"0\"..\"9\";\ntoken NUMBER = @ DIGIT+;\n\
         rule program = SOI expr EOI;\nrule atom = primary;\n\
         rule primary = value:NUMBER -> num;\n",
    );

    let out = nh()
        .args(["build"])
        .arg(&g)
        .arg("-o")
        .arg(dir.join("src/lang.pest"))
        .arg("--rust")
        .arg(dir.join("src"))
        .args(["--target", "nh-vm"])
        .output()
        .expect("running nh build");

    assert!(!out.status.success(), "an unexecutable role must fail the build");

    let err = String::from_utf8_lossy(&out.stderr);
    // `arrow` is the last honest gap: in a C-family grammar `->` is member
    // access, and this machine has no aggregate to reach into.
    assert!(err.contains("`arrow` role"), "names the role: {err}");
    assert!(err.contains("`->`"), "names a spelling the author typed: {err}");
    assert!(err.contains("nh-vm cannot execute"), "names the target: {err}");

    // And it must not leave a half-written file behind for a later build to
    // pick up and believe.
    assert!(
        !dir.join("src/generated/vm_operators.rs").exists(),
        "a failed target wrote a file anyway"
    );
}

#[test]
fn an_unknown_target_is_rejected_by_name() {
    let dir = scratch("unknown");
    let g = write(&dir, SUPPORTED);

    let out = nh()
        .args(["build"])
        .arg(&g)
        .arg("-o")
        .arg(dir.join("src/lang.pest"))
        .arg("--rust")
        .arg(dir.join("src"))
        .args(["--target", "jvm"])
        .output()
        .expect("running nh build");

    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("unknown target `jvm`"), "{err}");
    assert!(err.contains("nh-vm"), "says what is available: {err}");
}

/// `--target` only affects the Rust output, so passing it without `--rust`
/// asked for something and got nothing. It used to do so **silently**.
#[test]
fn target_without_rust_is_an_error_rather_than_a_no_op() {
    let dir = scratch("norust");
    let g = write(&dir, SUPPORTED);

    let out = nh()
        .args(["build"])
        .arg(&g)
        .arg("-o")
        .arg(dir.join("src/lang.pest"))
        .args(["--target", "nh-vm"])
        .output()
        .expect("running nh build");

    assert!(!out.status.success(), "a flag that does nothing must say so");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("no effect without") && err.contains("--rust"), "{err}");
}

/// The host type is a flag, not a hardcoded `crate::Interp`. A project that
/// calls its compiler something else used to get an impl for a type it does
/// not have.
#[test]
fn the_host_type_can_be_named() {
    let dir = scratch("host");
    let g = write(&dir, SUPPORTED);

    let out = nh()
        .args(["build"])
        .arg(&g)
        .arg("-o")
        .arg(dir.join("src/lang.pest"))
        .arg("--rust")
        .arg(dir.join("src"))
        .args(["--target", "nh-vm", "--host", "crate::Compiler"])
        .output()
        .expect("running nh build");

    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let generated = std::fs::read_to_string(dir.join("src/generated/vm_operators.rs")).unwrap();
    assert!(generated.contains("impl Operators for crate::Compiler {"), "{generated}");
}
