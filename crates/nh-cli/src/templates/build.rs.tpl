//! Regenerates `src/{{name}}.pest` and `src/generated/**` from `{{name}}.nh`.
//!
//! This is why you do not have to remember `nh build` after a grammar edit.
//! Cargo re-runs it whenever the `.nh` changes.
//!
//! It calls the `nh` binary rather than depending on the generator as a crate,
//! which is what keeps this project's dependencies down to pest.
//!
//! **`nh` is only needed to change the grammar.** The generated code is part of
//! this project, so somebody who clones it can build and run without installing
//! anything — right up until they edit the `.nh`, at which point the build stops
//! and says so rather than quietly compiling the old grammar.
//!
//! Safe on every build: handler files are never overwritten, and output is
//! byte-compared before writing so an unchanged grammar does not make cargo
//! rebuild everything.

use std::path::Path;
use std::process::Command;
use std::time::SystemTime;

const GRAMMAR: &str = "{{name}}.nh";
const PEST: &str = "src/{{name}}.pest";

fn main() {
    println!("cargo:rerun-if-changed={GRAMMAR}");

    let nh = std::env::var("NH").unwrap_or_else(|_| "nh".into());

    match Command::new(&nh)
        .args(["build", GRAMMAR, "-o", PEST, "--rust", "src"])
        .output()
    {
        Ok(out) if out.status.success() => {
            for line in String::from_utf8_lossy(&out.stderr).lines() {
                println!("cargo:warning={line}");
            }
        }
        Ok(out) => {
            for line in String::from_utf8_lossy(&out.stderr).lines() {
                println!("cargo:warning={line}");
            }
            panic!("`{nh} build` failed");
        }
        Err(_) => without_nh(&nh),
    }
}

/// What to do when `nh` is not installed.
///
/// Failing outright would mean a project cannot be built by anyone who does not
/// have the tool, which is wrong: the generated code is committed and compiles
/// on its own. Continuing *silently* would be worse, because an edited grammar
/// would compile as the previous one.
///
/// So: continue when the generated output is present and no older than the
/// grammar, and stop when it is missing or stale.
fn without_nh(nh: &str) {
    const HELP: &str = "help: install it with `cargo install --git \
                        https://github.com/Crash-Continuum-LLC/NailHammer nh-cli`, \
                        or set NH to its path";

    if !Path::new(PEST).exists() {
        panic!("`{nh}` is not available and `{PEST}` has not been generated yet.\n{HELP}");
    }

    if newer(GRAMMAR, PEST) {
        panic!(
            "`{GRAMMAR}` has changed but `{nh}` is not available to regenerate from it.\n\
             Building now would compile the previous grammar.\n{HELP}"
        );
    }

    println!(
        "cargo:warning=`{nh}` not found; using the generated code as committed. \
         Edits to {GRAMMAR} will not take effect until it is installed."
    );
}

fn newer(a: &str, b: &str) -> bool {
    fn at(p: &str) -> Option<SystemTime> {
        std::fs::metadata(p).ok()?.modified().ok()
    }
    match (at(a), at(b)) {
        (Some(a), Some(b)) => a > b,
        // Unknowable timestamps are not evidence of staleness.
        _ => false,
    }
}
