//! The runtime, embedded in the `nh` binary and written into each new project.
//!
//! A scaffolded project needs `nh-runtime` to compile. Getting it from a
//! registry means publishing; getting it from git means every user needs repo
//! access and `git-fetch-with-cli = true` before `cargo run` works at all. Both
//! are steps between somebody and a working project.
//!
//! So `nh init` writes the runtime into `vendor/nh-runtime/` and depends on it
//! by path. A generated project then needs **pest and nothing else** — no
//! credentials, no cargo configuration, no network beyond crates.io.
//!
//! The copy is pinned to the `nh` that generated it, which is the right
//! coupling rather than a limitation: generated code and its runtime have to
//! agree, and a floating dependency on `main` can break a project that has not
//! changed. Re-running `nh init` in a scratch directory is how you take a newer
//! runtime.

use std::io;
use std::path::Path;

/// `(path under vendor/nh-runtime/, contents)`.
///
/// Listed one by one rather than walked at build time, so adding a module to
/// `nh-runtime` without adding it here is a missing-file compile error rather
/// than a project that scaffolds and then does not build.
const SOURCES: &[(&str, &str)] = &[
    ("src/lib.rs", include_str!("../../nh-runtime/src/lib.rs")),
    ("src/ctx.rs", include_str!("../../nh-runtime/src/ctx.rs")),
    (
        "src/diagnostic.rs",
        include_str!("../../nh-runtime/src/diagnostic.rs"),
    ),
    ("src/error.rs", include_str!("../../nh-runtime/src/error.rs")),
    ("src/name.rs", include_str!("../../nh-runtime/src/name.rs")),
    ("src/node.rs", include_str!("../../nh-runtime/src/node.rs")),
    ("src/ops.rs", include_str!("../../nh-runtime/src/ops.rs")),
    (
        "src/source.rs",
        include_str!("../../nh-runtime/src/source.rs"),
    ),
];

/// A standalone manifest for the vendored copy.
///
/// `nh-runtime`'s own manifest inherits from the NailHammer workspace, which
/// does not exist in a scaffolded project. The empty `[workspace]` table is
/// what stops cargo adopting this crate into an enclosing workspace when
/// somebody scaffolds inside a monorepo.
fn manifest(version: &str) -> String {
    format!(
        r#"# Vendored by `nh init` from NailHammer {version}.
#
# This is the runtime the generated code compiles against. It is a copy rather
# than a dependency so that this project builds with no credentials and no
# cargo configuration.
#
# Do not edit: `nh init` writes it, and the generated code expects this exact
# version.
[package]
name = "nh-runtime"
version = "{version}"
edition = "2021"
license = "MIT"
publish = false

# Standalone: not a member of whatever workspace this project ends up inside.
[workspace]

[dependencies]
pest = "2.8"
"#
    )
}

/// Writes `vendor/nh-runtime/` under `root`.
pub fn write(root: &Path, version: &str) -> io::Result<Vec<String>> {
    let base = root.join("vendor").join("nh-runtime");
    let mut written = Vec::new();

    std::fs::create_dir_all(base.join("src"))?;

    std::fs::write(base.join("Cargo.toml"), manifest(version))?;
    written.push("vendor/nh-runtime/Cargo.toml".to_string());

    for (rel, contents) in SOURCES {
        std::fs::write(base.join(rel), contents)?;
        written.push(format!("vendor/nh-runtime/{rel}"));
    }

    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every module `lib.rs` declares must be vendored, or the copy does not
    /// compile. Checked against the real `lib.rs` rather than a list, so adding
    /// a module to the runtime and forgetting this file fails here.
    #[test]
    fn every_module_is_vendored() {
        let lib = SOURCES
            .iter()
            .find(|(p, _)| *p == "src/lib.rs")
            .expect("lib.rs is vendored")
            .1;

        let declared: Vec<&str> = lib
            .lines()
            .filter_map(|l| l.trim().strip_prefix("pub mod "))
            .filter_map(|l| l.strip_suffix(';'))
            .collect();

        assert!(!declared.is_empty(), "lib.rs declares no modules?");

        for m in declared {
            let want = format!("src/{m}.rs");
            assert!(
                SOURCES.iter().any(|(p, _)| *p == want),
                "`{m}` is declared in nh-runtime's lib.rs but not vendored; \
                 add it to SOURCES in crates/nh-cli/src/vendor.rs"
            );
        }
    }

    /// The vendored manifest must not inherit from a workspace that will not
    /// be there.
    #[test]
    fn the_manifest_is_standalone() {
        let m = manifest("0.1.0");
        assert!(!m.contains(".workspace = true"), "{m}");
        assert!(m.contains("[workspace]"), "must not be adopted by a parent:\n{m}");
        assert!(m.contains("pest = "), "{m}");
    }

    /// Nothing vendored may reference the NailHammer workspace.
    #[test]
    fn the_sources_are_self_contained() {
        for (path, src) in SOURCES {
            assert!(
                !src.contains("nh_syntax") && !src.contains("nh_codegen"),
                "{path} reaches outside the runtime"
            );
        }
    }
}
