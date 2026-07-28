//! Regenerates `src/config.pest` and `src/generated/**` from `config.nh`.
//!
//! Without this you have to remember `nh build` after every grammar edit, and
//! forgetting means compiling against stale views.
//!
//! Safe to run on every build: handler files are never overwritten, and output
//! is byte-compared before writing so cargo does not rebuild the world.

fn main() {
    nh_build::Builder::new("config.nh").run();
}
