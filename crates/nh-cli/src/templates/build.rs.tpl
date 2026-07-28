//! Regenerates `src/{{name}}.pest` and `src/generated/**` from `{{name}}.nh`.
//!
//! This is why you do not have to remember `nh build` after a grammar edit.
//! Cargo re-runs it whenever the `.nh` changes.
//!
//! Safe on every build: handler files are never overwritten, and output is
//! byte-compared before writing so an unchanged grammar does not make cargo
//! rebuild everything.

fn main() {
    nh_build::Builder::new("{{name}}.nh").run();
}
