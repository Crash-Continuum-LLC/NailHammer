//! This crate's own source, as text, for `nh init` to copy into a project.
//!
//! `nh init` writes the runtime into `vendor/nh-runtime/` and depends on it by
//! path, so a scaffolded project needs **pest and nothing else** — no
//! credentials, no cargo configuration, no registry. Doing that means the `nh`
//! binary has to carry the runtime's source text, and the text has to come from
//! somewhere.
//!
//! It comes from here, rather than from `nh-cli` reaching across the workspace
//! with `include_str!("../../nh-runtime/src/lib.rs")`, because that path only
//! exists in a checkout. Packaged for a registry, `nh-cli` is a tarball with no
//! sibling directories, and every one of those includes fails to compile —
//! which is exactly what publishing to crates.io turned up. A crate can always
//! read its own files, so the table lives with the files it names.
//!
//! Behind the `vendor` feature because only `nh-cli` needs it. Without the gate
//! every scaffolded project would carry a copy of the runtime's source inside
//! the runtime it already has.

/// `(path under vendor/nh-runtime/, contents)`.
///
/// Listed one by one rather than walked at build time, so adding a module to
/// this crate without adding it here is a missing-file compile error rather
/// than a project that scaffolds and then does not build.
pub const SOURCES: &[(&str, &str)] = &[
    ("src/lib.rs", include_str!("lib.rs")),
    ("src/ctx.rs", include_str!("ctx.rs")),
    ("src/diagnostic.rs", include_str!("diagnostic.rs")),
    ("src/error.rs", include_str!("error.rs")),
    ("src/name.rs", include_str!("name.rs")),
    ("src/node.rs", include_str!("node.rs")),
    ("src/ops.rs", include_str!("ops.rs")),
    ("src/shared.rs", include_str!("shared.rs")),
    ("src/source.rs", include_str!("source.rs")),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Every module `lib.rs` declares must be vendored, or the copy does not
    /// compile. Checked against the real `lib.rs` rather than a list, so adding
    /// a module to this crate and forgetting this file fails here.
    ///
    /// `#[cfg]`-gated modules are skipped: this module is one, and it is
    /// deliberately not vendored — a scaffolded project has no use for the
    /// runtime's source inside the runtime.
    #[test]
    fn every_module_is_vendored() {
        let lib = SOURCES
            .iter()
            .find(|(p, _)| *p == "src/lib.rs")
            .expect("lib.rs is vendored")
            .1;

        let mut gated = false;
        let mut declared = Vec::new();
        for line in lib.lines() {
            let line = line.trim();
            if line.starts_with("#[cfg(") {
                gated = true;
                continue;
            }
            if let Some(m) = line.strip_prefix("pub mod ").and_then(|l| l.strip_suffix(';')) {
                if !gated {
                    declared.push(m);
                }
            }
            if !line.is_empty() {
                gated = false;
            }
        }

        assert!(!declared.is_empty(), "lib.rs declares no ungated modules?");

        for m in declared {
            let want = format!("src/{m}.rs");
            assert!(
                SOURCES.iter().any(|(p, _)| *p == want),
                "`{m}` is declared in lib.rs but not vendored; \
                 add it to SOURCES in crates/nh-runtime/src/vendor.rs"
            );
        }
    }

    /// The vendored copy must not be the thing that drags the toolkit into a
    /// user's project.
    #[test]
    fn the_sources_are_self_contained() {
        for (path, src) in SOURCES {
            assert!(
                !src.contains("nh_syntax") && !src.contains("nh_codegen"),
                "{path} reaches outside the runtime"
            );
        }
    }

    /// `vendor.rs` itself must stay out of the table. Including it would make
    /// the vendored copy contain `include_str!` calls for files it has, plus
    /// one for a module the generated project never enables.
    #[test]
    fn this_module_is_not_vendored() {
        assert!(
            !SOURCES.iter().any(|(p, _)| *p == "src/vendor.rs"),
            "vendor.rs must not vendor itself"
        );
    }
}
