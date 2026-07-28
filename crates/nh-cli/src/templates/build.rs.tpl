//! Regenerates `src/{{name}}.pest` and `src/generated/**` from `{{name}}.nh`.
//!
//! This is why you do not have to remember `nh build` after a grammar edit.
//! Cargo re-runs it whenever the `.nh` changes.
//!
//! It calls the `nh` binary rather than depending on the generator as a crate,
//! which is what keeps this project's dependencies down to pest. If `nh` is not
//! on your PATH the build says so and stops, rather than compiling against a
//! stale grammar.
//!
//! Safe on every build: handler files are never overwritten, and output is
//! byte-compared before writing so an unchanged grammar does not make cargo
//! rebuild everything.

use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed={{name}}.nh");

    let nh = std::env::var("NH").unwrap_or_else(|_| "nh".into());

    let out = Command::new(&nh)
        .args([
            "build",
            "{{name}}.nh",
            "-o",
            "src/{{name}}.pest",
            "--rust",
            "src",
        ])
        .output();

    match out {
        Ok(o) if o.status.success() => {
            // `nh` reports what it wrote on stderr; surface it under `-vv`.
            for line in String::from_utf8_lossy(&o.stderr).lines() {
                println!("cargo:warning={line}");
            }
        }
        Ok(o) => {
            for line in String::from_utf8_lossy(&o.stderr).lines() {
                println!("cargo:warning={line}");
            }
            panic!("`{} build` failed", nh);
        }
        Err(e) => panic!(
            "cannot run `{nh}`: {e}\n\
             help: install it with `cargo install --git \
             https://github.com/Crash-Continuum-LLC/NailHammer nh-cli`, \
             or set NH to its path"
        ),
    }
}
