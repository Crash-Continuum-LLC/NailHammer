//! `build.rs` integration.
//!
//! Without this, keeping generated code current is a manual `nh build` you have
//! to remember after every grammar edit — and forgetting means compiling
//! against stale views, which fails confusingly or, worse, succeeds.
//!
//! ```no_run
//! // build.rs
//! nh_build::Builder::new("mylang.nh").run();
//! ```
//!
//! Two things make this safe to run on every build:
//!
//! * **Handler files are never overwritten** — the same policy `nh build --rust`
//!   uses (DESIGN.md §5.4). A build that clobbered hand-written code would be
//!   unusable.
//! * **Output is byte-compared before writing.** Rewriting an unchanged file
//!   would update its mtime and make cargo rebuild the world every time.

use std::path::{Path, PathBuf};

use nh_syntax::SourceMap;

/// Configures generation. Paths are relative to the crate root, the directory
/// `cargo` runs `build.rs` from.
pub struct Builder {
    root: PathBuf,
    grammar: PathBuf,
    pest_out: Option<PathBuf>,
    rust_out: Option<PathBuf>,
    deny_warnings: bool,
}

impl Builder {
    /// Generates from `grammar`, writing `.pest` and Rust into `src/`.
    pub fn new(grammar: impl Into<PathBuf>) -> Self {
        Builder {
            root: PathBuf::from("."),
            grammar: grammar.into(),
            pest_out: None,
            rust_out: Some(PathBuf::from("src")),
            deny_warnings: false,
        }
    }

    /// Resolves relative paths against `root` instead of the current directory.
    ///
    /// Defaults to `.`, which is where cargo runs a build script. Set it
    /// explicitly when calling from somewhere else — depending on the process
    /// working directory is not safe when several builds run concurrently.
    pub fn root(mut self, path: impl Into<PathBuf>) -> Self {
        self.root = path.into();
        self
    }

    /// Writes the `.pest` here instead of `src/<stem>.pest`.
    pub fn pest_output(mut self, path: impl Into<PathBuf>) -> Self {
        self.pest_out = Some(path.into());
        self
    }

    /// Writes generated Rust here instead of `src/`.
    pub fn rust_output(mut self, path: impl Into<PathBuf>) -> Self {
        self.rust_out = Some(path.into());
        self
    }

    /// Emits only the `.pest`, no Rust.
    pub fn without_rust(mut self) -> Self {
        self.rust_out = None;
        self
    }

    /// Fails the build on determinism warnings as well as errors.
    ///
    /// Off by default: a warning should not stop you compiling while you are
    /// mid-edit. Turn it on to hold the line in CI.
    pub fn deny_warnings(mut self, deny: bool) -> Self {
        self.deny_warnings = deny;
        self
    }

    /// Runs generation, panicking with a rendered diagnostic on failure.
    ///
    /// Panicking is right here: `build.rs` has no other way to fail a build, and
    /// cargo shows the message.
    pub fn run(self) {
        if let Err(message) = self.try_run() {
            // A blank line first: cargo prefixes build-script output densely and
            // the diagnostic is easier to read with room around it.
            panic!("\n\n{message}");
        }
    }

    /// Like [`Builder::run`] but returns the rendered error instead of
    /// panicking, for callers that want to decide.
    pub fn try_run(self) -> Result<Generated, String> {
        let mut sources = SourceMap::new();
        let grammar = self.root.join(&self.grammar);

        // Tell cargo to re-run when the grammar changes. Without this, editing
        // the `.nh` would not trigger regeneration and you would compile
        // against stale generated code.
        rerun_if_changed(&grammar);

        let ast = nh_syntax::resolve(&mut sources, &grammar)
            .map_err(|e| e.render(&sources))?;

        // Imported grammars are inputs too.
        for import in &ast.imports {
            let base = grammar.parent().unwrap_or(Path::new("."));
            rerun_if_changed(&base.join(&import.path.value));
        }

        let table = nh_operators::resolve(&ast, &mut sources).map_err(|e| e.render(&sources))?;

        let diagnostics = nh_analysis::analyse(&ast, table.atom_rule.as_deref());
        let errors = diagnostics
            .iter()
            .filter(|d| d.severity == nh_syntax::Severity::Error)
            .count();
        let warnings = diagnostics.len() - errors;

        // Warnings go through cargo's own channel so they surface in build
        // output rather than being swallowed.
        for d in &diagnostics {
            for line in d.render(&sources).lines() {
                println!("cargo:warning={line}");
            }
        }
        if errors > 0 || (self.deny_warnings && warnings > 0) {
            return Err(format!(
                "{} error(s), {} warning(s) in {}",
                errors,
                warnings,
                grammar.display()
            ));
        }

        let lowered = nh_lower::lower(&ast, &table).map_err(|e| e.render(&sources))?;

        let mut out = Generated::default();

        let pest_path = self.root.join(self.pest_out.clone().unwrap_or_else(|| {
            let stem = self
                .grammar
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "grammar".to_string());
            PathBuf::from("src").join(format!("{stem}.pest"))
        }));
        if write_if_changed(&pest_path, &lowered.pest)? {
            out.written.push(pest_path);
        }

        if let Some(rust_out) = &self.rust_out {
            let root = self.root.join(rust_out);
            let opts = nh_codegen::Options::default();
            let generated = nh_codegen::generate(&ast, &table, &lowered, &opts);

            for file in &generated.files {
                let path = root.join(&file.path);
                match file.policy {
                    nh_codegen::Policy::Generated => {
                        if write_if_changed(&path, &file.contents)? {
                            out.written.push(path);
                        }
                    }
                    // Never overwritten. A build script that clobbered
                    // hand-written handlers would be unusable.
                    nh_codegen::Policy::OnceOnly => {
                        if path.exists() {
                            out.kept.push(path);
                        } else if write_if_changed(&path, &file.contents)? {
                            out.created.push(path);
                        }
                    }
                }
            }

            // A handler that no longer matches its grammar alternative. The
            // compiler catches arity and type changes; it cannot catch a
            // rename or a reorder, because parameters are positional.
            out.drift = nh_codegen::drift::check_all(&lowered, |rel| {
                std::fs::read_to_string(root.join(rel)).ok()
            })
            .into_iter()
            .map(|(alt, d)| DriftReport {
                path: root.join(format!("handlers/{}.rs", alt.pest_rule)),
                message: d.message(&format!("handlers/{}.rs", alt.pest_rule)),
                is_error: d.is_error(),
            })
            .collect();
        }

        Ok(out)
    }
}

/// A handler file that disagrees with the grammar.
#[derive(Debug)]
pub struct DriftReport {
    pub path: PathBuf,
    pub message: String,
    /// True when the handler is now *wrong*, not merely misnamed.
    pub is_error: bool,
}

#[derive(Debug, Default)]
pub struct Generated {
    /// Files written or rewritten because their contents changed.
    pub written: Vec<PathBuf>,
    /// Handler stubs created because none existed.
    pub created: Vec<PathBuf>,
    /// Handler files left alone.
    pub kept: Vec<PathBuf>,
    /// Handlers whose parameters no longer match their grammar alternative.
    pub drift: Vec<DriftReport>,
}

fn rerun_if_changed(path: &Path) {
    println!("cargo:rerun-if-changed={}", path.display());
}

/// Writes only when the contents differ.
///
/// Rewriting an identical file bumps its mtime, which makes cargo rebuild
/// everything downstream on every single build.
fn write_if_changed(path: &Path, contents: &str) -> Result<bool, String> {
    if let Ok(existing) = std::fs::read_to_string(path) {
        if existing == contents {
            return Ok(false);
        }
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create `{}`: {e}", parent.display()))?;
    }
    std::fs::write(path, contents)
        .map_err(|e| format!("cannot write `{}`: {e}", path.display()))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unchanged_file_is_not_rewritten() {
        let dir = std::env::temp_dir().join("nh-build-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("stable-{}.txt", std::process::id()));

        assert!(write_if_changed(&path, "hello").unwrap(), "first write");
        assert!(
            !write_if_changed(&path, "hello").unwrap(),
            "identical contents must not touch the file, or cargo rebuilds everything"
        );
        assert!(write_if_changed(&path, "goodbye").unwrap(), "changed contents");
    }
}
