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

/// `(path under vendor/nh-runtime/, contents)`, owned by the runtime.
///
/// This used to be an `include_str!` table reaching across the workspace into
/// `../../nh-runtime/src/`. That path exists in a checkout and nowhere else:
/// packaged for a registry, this crate is a tarball with no sibling
/// directories, and every include failed to compile. A crate can always read
/// its own files, so the table lives in `nh-runtime` behind its `vendor`
/// feature and this crate reads it from there.
use nh_runtime::vendor::SOURCES;

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

# `Arc` instead of `Rc` for the generated AST, so a program tree is `Send +
# Sync`. Turn it on where this crate is depended on:
#
#     nh-runtime = {{ path = "vendor/nh-runtime", features = ["threadsafe"] }}
#
# Nothing else changes — the generated code and your handlers both say
# `Shared<T>`, so no signature moves. See `src/shared.rs`.
[features]
threadsafe = []

# Declared, never enabled. Upstream, `vendor` exposes the runtime's source text
# so `nh init` can write this directory; the module that does it is the one
# file deliberately not copied here, because a vendored runtime has no use for
# a second copy of itself. The feature has to be *declared* anyway: `lib.rs`
# still carries `#[cfg(feature = "vendor")]`, and cargo warns about a cfg on a
# feature the manifest does not know about. A warning in generated code is a
# defect in the generator, so the manifest knows about it.
#
# Do not turn it on: `src/vendor.rs` is not here.
vendor = []
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

    /// Every `#[cfg(feature = "..")]` in the vendored source must name a
    /// feature the vendored manifest declares.
    ///
    /// Cargo warns on a cfg referring to a feature the manifest does not know
    /// about, and that warning surfaces in the user's project, about a file
    /// the user did not write and cannot fix. Adding a gated module to
    /// `nh-runtime` and not declaring it here is the way that happens — it is
    /// how `vendor` itself got through the first time.
    #[test]
    fn every_gated_feature_is_declared_in_the_manifest() {
        let m = manifest("0.2.0");

        for (path, src) in SOURCES {
            for line in src.lines() {
                let Some(rest) = line.trim().strip_prefix("#[cfg(feature = \"") else {
                    continue;
                };
                let Some(feature) = rest.split('"').next() else {
                    continue;
                };
                assert!(
                    m.contains(&format!("\n{feature} = [")),
                    "{path} gates on feature `{feature}`, which the vendored \
                     manifest does not declare — the generated project will \
                     warn about an unexpected cfg"
                );
            }
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

    /// What `write` actually produces is a manifest plus every entry in the
    /// table. The table's own invariants — that it covers each module `lib.rs`
    /// declares, and that nothing in it reaches back into the toolkit — are
    /// tested where the table lives, in `nh-runtime`'s `vendor` module.
    #[test]
    fn write_produces_a_manifest_and_every_source() {
        let dir = std::env::temp_dir().join(format!("nh-vendor-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let written = write(&dir, "0.2.0").expect("vendoring writes");

        assert!(written.contains(&"vendor/nh-runtime/Cargo.toml".to_string()));
        assert_eq!(
            written.len(),
            SOURCES.len() + 1,
            "one manifest plus every source: {written:?}"
        );
        for (rel, _) in SOURCES {
            let path = dir.join("vendor/nh-runtime").join(rel);
            assert!(path.is_file(), "{rel} was not written");
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}
