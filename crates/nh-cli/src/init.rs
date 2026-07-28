//! `nh init` — project scaffolding.
//!
//! The point is not convenience, it is **encoding the things that are expected
//! but not obvious**. A user who skims the guide will still get a project that
//! anchors its entry rule with `SOI`/`EOI` and enables `pest_derive`'s
//! `grammar-extras` feature, because both are already in the generated files.
//!
//! Both of those failures are silent. A grammar without `SOI` parses fine until
//! someone puts a blank line at the top of a file; a build without
//! `grammar-extras` compiles, parses, and returns `None` from every tag lookup
//! forever. Neither produces an error message pointing at the cause, which is
//! exactly why they belong in a template rather than in prose.

use std::path::{Path, PathBuf};

/// Files are stored as real templates so they can be read, linted, and — most
/// importantly — exercised by the test suite that scaffolds a project and then
/// parses its own sample program.
const GRAMMAR: &str = include_str!("templates/grammar.nh");
const CARGO_TOML: &str = include_str!("templates/Cargo.toml");
const LIB_RS: &str = include_str!("templates/lib.rs");
const MAIN_RS: &str = include_str!("templates/main.rs");
const README: &str = include_str!("templates/README.md");
const GITIGNORE: &str = include_str!("templates/gitignore");
const BUILD_RS: &str = include_str!("templates/build.rs.tpl");
const SAMPLE: &str = include_str!("templates/sample");

/// Hand-written handlers, one per grammar alternative.
///
/// The scaffold ships *working* handlers rather than the `todo!` stubs
/// `nh build --rust` would create, so `cargo run` does something on the first
/// try. `nh build --rust` never overwrites an existing handler, so these
/// survive regeneration.
const HANDLERS: &[(&str, &str)] = &[
    ("program", include_str!("templates/handlers/program.rs")),
    ("stmt_bind", include_str!("templates/handlers/stmt_bind.rs")),
    ("stmt_print", include_str!("templates/handlers/stmt_print.rs")),
    ("stmt_eval", include_str!("templates/handlers/stmt_eval.rs")),
    ("primary_num", include_str!("templates/handlers/primary_num.rs")),
    ("primary_var", include_str!("templates/handlers/primary_var.rs")),
];

/// Whether the NailHammer crates are on crates.io.
///
/// **Flip this to `true` at publish time — it is the only edit needed.**
///
/// While it is `false`, a scaffolded project depends on the crates by path, so
/// it points into whatever checkout built the `nh` binary and only builds on
/// that machine. Switching to a plain `version` before the crates actually
/// exist would make every scaffolded project fail to build instead, which is
/// worse, so the order matters: publish first, then flip.
const PUBLISHED: bool = false;

/// How a scaffolded `Cargo.toml` should depend on one of our crates.
fn dependency(crate_name: &str) -> String {
    if PUBLISHED {
        format!("\"{VERSION}\"")
    } else {
        format!(
            "{{ version = \"{VERSION}\", path = \"{}\" }}",
            crate_path(crate_name)
        )
    }
}

/// Kept in step with the workspace version.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Path to this checkout's `nh-runtime`.
///
/// `nh-runtime` is not published, so a scaffolded project has to point at the
/// crate inside whatever checkout built the `nh` binary. Captured at compile
/// time, since at run time `nh` has no idea where it came from.
fn crate_path(name: &str) -> String {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .map(|crates| crates.join(name))
        .unwrap_or_else(|| PathBuf::from("..").join(name))
        .display()
        .to_string()
}

#[derive(Debug)]
pub struct Options {
    pub dir: PathBuf,
    /// Crate and file stem, e.g. `mylang`.
    pub name: String,
    /// Grammar name, e.g. `Mylang`.
    pub grammar: String,
    /// Source file extension for the target language.
    pub ext: String,
    pub force: bool,
}

impl Options {
    pub fn new(dir: PathBuf, name: Option<String>, ext: Option<String>, force: bool) -> Result<Self, String> {
        let derived = match &name {
            Some(n) => n.clone(),
            None => dir
                .canonicalize()
                .ok()
                .as_deref()
                .and_then(Path::file_name)
                .or_else(|| dir.file_name())
                .map(|s| s.to_string_lossy().into_owned())
                .ok_or("cannot infer a project name; pass --name")?,
        };

        let name = sanitize(&derived);
        if name.is_empty() {
            return Err(format!("`{derived}` is not a usable project name; pass --name"));
        }
        // A crate named after a Rust keyword generates `use pub::...`, which
        // does not compile. Better to say so than to rename behind their back.
        if is_rust_keyword(&name) {
            return Err(format!(
                "`{name}` is a Rust keyword, so it cannot name a crate; pass --name"
            ));
        }
        let grammar = pascal_case(&name);
        let ext = ext.unwrap_or_else(|| name.clone());

        Ok(Options {
            dir,
            name,
            grammar,
            ext,
            force,
        })
    }
}

/// Keywords that cannot appear in a `use` path, and so cannot name a crate.
fn is_rust_keyword(s: &str) -> bool {
    const KEYWORDS: &[&str] = &[
        "as", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "false",
        "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
        "ref", "return", "self", "static", "struct", "super", "trait", "true", "type", "unsafe",
        "use", "where", "while", "async", "await", "box", "final", "macro", "override", "priv",
        "try", "typeof", "unsized", "virtual", "yield",
    ];
    KEYWORDS.contains(&s)
}

/// Lowercases and replaces anything that is not a valid Rust identifier
/// character, so `My Lang-2` becomes `my_lang_2`.
fn sanitize(s: &str) -> String {
    let mut out: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    out = out.trim_matches('_').to_string();
    if out.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    out
}

fn pascal_case(s: &str) -> String {
    s.split('_')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut c = p.chars();
            match c.next() {
                Some(f) => f.to_ascii_uppercase().to_string() + c.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

fn render(template: &str, opts: &Options) -> String {
    template
        .replace("{{name}}", &opts.name)
        .replace("{{Name}}", &opts.grammar)
        .replace("{{ext}}", &opts.ext)
        .replace("{{runtimedep}}", &dependency("nh-runtime"))
        .replace("{{builddep}}", &dependency("nh-build"))
}

pub struct Created {
    pub files: Vec<PathBuf>,
    pub grammar_path: PathBuf,
    pub pest_path: PathBuf,
}

/// Writes the scaffold. Does not generate the `.pest` — the caller does that by
/// running the real lowering pipeline, so a scaffolded project is verified to
/// build by the same code path a user would use.
pub fn scaffold(opts: &Options) -> Result<Created, String> {
    if opts.dir.exists() && !opts.force {
        let occupied = std::fs::read_dir(&opts.dir)
            .map_err(|e| format!("cannot read `{}`: {e}", opts.dir.display()))?
            .flatten()
            .any(|e| !e.file_name().to_string_lossy().starts_with('.'));
        if occupied {
            return Err(format!(
                "`{}` is not empty; pass --force to write into it anyway",
                opts.dir.display()
            ));
        }
    }

    let src = opts.dir.join("src");
    std::fs::create_dir_all(&src)
        .map_err(|e| format!("cannot create `{}`: {e}", src.display()))?;

    let grammar_path = opts.dir.join(format!("{}.nh", opts.name));
    let pest_path = src.join(format!("{}.pest", opts.name));

    let handlers = src.join("handlers");
    std::fs::create_dir_all(&handlers)
        .map_err(|e| format!("cannot create `{}`: {e}", handlers.display()))?;

    let mut files = vec![
        (grammar_path.clone(), render(GRAMMAR, opts)),
        (opts.dir.join("Cargo.toml"), render(CARGO_TOML, opts)),
        (opts.dir.join("README.md"), render(README, opts)),
        (opts.dir.join(".gitignore"), render(GITIGNORE, opts)),
        (
            opts.dir.join(format!("sample.{}", opts.ext)),
            render(SAMPLE, opts),
        ),
        (opts.dir.join("build.rs"), render(BUILD_RS, opts)),
        (src.join("lib.rs"), render(LIB_RS, opts)),
        (src.join("main.rs"), render(MAIN_RS, opts)),
    ];
    for (name, body) in HANDLERS {
        files.push((handlers.join(format!("{name}.rs")), render(body, opts)));
    }

    let mut written = Vec::new();
    for (path, contents) in files {
        std::fs::write(&path, contents)
            .map_err(|e| format!("cannot write `{}`: {e}", path.display()))?;
        written.push(path);
    }

    Ok(Created {
        files: written,
        grammar_path,
        pest_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_sanitised() {
        assert_eq!(sanitize("mylang"), "mylang");
        assert_eq!(sanitize("My Lang-2"), "my_lang_2");
        assert_eq!(sanitize("-weird-"), "weird");
        assert_eq!(sanitize("9lives"), "_9lives");
    }

    /// Both dependency forms must be well-formed TOML, so flipping `PUBLISHED`
    /// at publish time cannot produce a manifest that does not parse.
    #[test]
    fn both_dependency_forms_are_valid() {
        let path_form = format!(
            "{{ version = \"{VERSION}\", path = \"{}\" }}",
            crate_path("nh-runtime")
        );
        assert!(path_form.starts_with("{ version = "), "{path_form}");
        assert!(path_form.ends_with(" }"), "{path_form}");

        let version_form = format!("\"{VERSION}\"");
        assert_eq!(version_form, "\"0.1.0\"");

        // Whichever is active must be what `dependency` returns.
        let active = dependency("nh-runtime");
        assert_eq!(active, if PUBLISHED { version_form } else { path_form });
    }

    #[test]
    fn a_keyword_name_is_refused() {
        // `nh init pub` would generate `use pub::...`, which does not compile.
        let err = Options::new(PathBuf::from("/tmp/pub"), Some("pub".into()), None, false)
            .expect_err("a Rust keyword cannot name a crate");
        assert!(err.contains("Rust keyword"), "{err}");
        assert!(err.contains("--name"), "{err}");
    }

    #[test]
    fn grammar_names_are_pascal_case() {
        assert_eq!(pascal_case("mylang"), "Mylang");
        assert_eq!(pascal_case("my_lang_2"), "MyLang2");
    }

    #[test]
    fn templates_have_no_unreplaced_placeholders() {
        let opts = Options::new(PathBuf::from("/tmp/x"), Some("demo".into()), None, false).unwrap();
        for t in [GRAMMAR, CARGO_TOML, MAIN_RS, README, GITIGNORE, SAMPLE] {
            let out = render(t, &opts);
            assert!(
                !out.contains("{{"),
                "unreplaced placeholder in template:\n{out}"
            );
        }
    }
}
