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

use crate::features::{Feature, Features, Style, ARG_RULE};

/// Files are stored as real templates so they can be read, linted, and — most
/// importantly — exercised by the test suite that scaffolds a project and then
/// parses its own sample program.
const GRAMMAR_C: &str = include_str!("templates/grammar_c.nh");
const GRAMMAR_BASIC: &str = include_str!("templates/grammar_basic.nh");
// `.tpl`, not `Cargo.toml`. Cargo scans for manifests wherever this crate is
// checked out and tries to parse anything with that name — including a template
// full of `{{placeholder}}` — so an unsuffixed one prints a parse error on every
// build of every project that depends on us. Same reason as `build.rs.tpl`.
const CARGO_TOML: &str = include_str!("templates/Cargo.toml.tpl");
/// One host per (shape, style). They duplicate a good deal of each other on
/// purpose: a template is read and edited by whoever scaffolded it, so being
/// self-contained matters more than being factored.
const LIB_RS: &str = include_str!("templates/lib.rs");
const LIB_RS_BASIC: &str = include_str!("templates/lib_basic.rs");
const LIB_RS_COMPILER: &str = include_str!("templates/lib_compiler.rs");
const LIB_RS_COMPILER_BASIC: &str = include_str!("templates/lib_compiler_basic.rs");

fn sample_template(style: Style) -> &'static str {
    match style {
        Style::C => SAMPLE_C,
        Style::Basic => SAMPLE_BASIC,
    }
}

fn lib_template(opts: &Options) -> &'static str {
    match (opts.is_compiler, opts.style) {
        (false, Style::C) => LIB_RS,
        (false, Style::Basic) => LIB_RS_BASIC,
        (true, Style::C) => LIB_RS_COMPILER,
        (true, Style::Basic) => LIB_RS_COMPILER_BASIC,
    }
}
const MAIN_RS: &str = include_str!("templates/main.rs");
const README: &str = include_str!("templates/README.md");
const GITIGNORE: &str = include_str!("templates/gitignore");
const BUILD_RS: &str = include_str!("templates/build.rs.tpl");
const SAMPLE_C: &str = include_str!("templates/sample_c");
const SAMPLE_BASIC: &str = include_str!("templates/sample_basic");
const SAMPLE_LOOPS_C: &str = include_str!("templates/sample_loops_c");
const SAMPLE_LOOPS_BASIC: &str = include_str!("templates/sample_loops_basic");
const SAMPLE_FNS_C: &str = include_str!("templates/sample_functions_c");
const SAMPLE_FNS_BASIC: &str = include_str!("templates/sample_functions_basic");

/// Hand-written handlers, one per grammar alternative.
///
/// The scaffold ships *working* handlers rather than the `todo!` stubs
/// `nh build --rust` would create, so `cargo run` does something on the first
/// try. `nh build --rust` never overwrites an existing handler, so these
/// survive regeneration.
const HANDLERS: &[(&str, &str)] = &[
    ("program", include_str!("templates/handlers/program.rs")),
    ("block", include_str!("templates/handlers/block.rs")),
    ("line", include_str!("templates/handlers/line.rs")),
    ("stmt_bind", include_str!("templates/handlers/stmt_bind.rs")),
    ("stmt_print", include_str!("templates/handlers/stmt_print.rs")),
    ("stmt_iff", include_str!("templates/handlers/stmt_iff.rs")),
    ("else_tail", include_str!("templates/handlers/else_tail.rs")),
    ("stmt_eval", include_str!("templates/handlers/stmt_eval.rs")),
    ("stmt_while", include_str!("templates/handlers/stmt_while.rs")),
    ("stmt_for", include_str!("templates/handlers/stmt_for.rs")),
    ("stmt_do", include_str!("templates/handlers/stmt_do.rs")),
    ("stmt_break", include_str!("templates/handlers/stmt_break.rs")),
    ("stmt_continue", include_str!("templates/handlers/stmt_continue.rs")),
    ("stmt_fn", include_str!("templates/handlers/stmt_fn.rs")),
    ("stmt_return", include_str!("templates/handlers/stmt_return.rs")),
    ("primary_num", include_str!("templates/handlers/primary_num.rs")),
    ("primary_var", include_str!("templates/handlers/primary_var.rs")),
    ("primary_call", include_str!("templates/handlers/primary_call.rs")),
    ("param_list", include_str!("templates/handlers/param_list.rs")),
    ("more_param", include_str!("templates/handlers/more_param.rs")),
    ("more_arg", include_str!("templates/handlers/more_arg.rs")),
];

/// The same alternatives for a host that emits rather than computes.
///
/// Compare them side by side: the signatures are identical bar the return type,
/// and every body does the emitting equivalent of what the interpreter's does.
/// That similarity is the claim `--compiler` exists to make checkable.
const HANDLERS_COMPILER: &[(&str, &str)] = &[
    ("program", include_str!("templates/handlers_compiler/program.rs")),
    ("block", include_str!("templates/handlers_compiler/block.rs")),
    ("line", include_str!("templates/handlers_compiler/line.rs")),
    ("stmt_bind", include_str!("templates/handlers_compiler/stmt_bind.rs")),
    ("stmt_print", include_str!("templates/handlers_compiler/stmt_print.rs")),
    ("stmt_iff", include_str!("templates/handlers_compiler/stmt_iff.rs")),
    ("else_tail", include_str!("templates/handlers_compiler/else_tail.rs")),
    ("stmt_eval", include_str!("templates/handlers_compiler/stmt_eval.rs")),
    ("stmt_while", include_str!("templates/handlers_compiler/stmt_while.rs")),
    ("stmt_for", include_str!("templates/handlers_compiler/stmt_for.rs")),
    ("stmt_do", include_str!("templates/handlers_compiler/stmt_do.rs")),
    ("stmt_break", include_str!("templates/handlers_compiler/stmt_break.rs")),
    ("stmt_continue", include_str!("templates/handlers_compiler/stmt_continue.rs")),
    ("stmt_fn", include_str!("templates/handlers_compiler/stmt_fn.rs")),
    ("stmt_return", include_str!("templates/handlers_compiler/stmt_return.rs")),
    ("primary_num", include_str!("templates/handlers_compiler/primary_num.rs")),
    ("primary_var", include_str!("templates/handlers_compiler/primary_var.rs")),
    ("primary_call", include_str!("templates/handlers_compiler/primary_call.rs")),
    ("param_list", include_str!("templates/handlers_compiler/param_list.rs")),
    ("more_param", include_str!("templates/handlers_compiler/more_param.rs")),
    ("more_arg", include_str!("templates/handlers_compiler/more_arg.rs")),
];

/// What `main.rs` does with what the run produced. The only part of the binary
/// that knows which shape this project is — everything else, including the
/// whole error path, is identical.
const PRODUCED_INTERP: &str = r#"    for line in &interp.output {
        println!("{line}");
    }"#;

const PRODUCED_COMPILER: &str = r#"    // Compiling produced instructions; running them produces output.
    eprintln!("--- bytecode ---");
    for (i, op) in interp.code.iter().enumerate() {
        eprintln!("{i:3}  {op:?}");
    }
    eprintln!("--- output ---");
    for line in interp.run() {
        println!("{line}");
    }"#;

/// Added to `Cargo.toml` by `--async`.
const TOKIO_DEP: &str = r#"
# Added by `nh init --async`. `rt-multi-thread` is not optional: the helper in
# `src/lib.rs` uses `block_in_place`, which panics on the current-thread
# runtime.
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
"#;

/// The helper `--async` adds to the host, and the reason it exists.
const ASYNC_SUPPORT: &str = r#"
impl Interp {
    /// Runs a future from inside a handler.
    ///
    /// Handlers are synchronous, because the evaluator is: making it async
    /// would mean every `eval_*` returned a boxed future — a heap allocation
    /// per node — whether or not a language ever awaits anything.
    ///
    /// So async work is *blocked on* instead. The obvious spelling of that,
    ///
    /// ```ignore
    /// Handle::current().block_on(fut)   // panics
    /// ```
    ///
    /// fails with "Cannot start a runtime from within a runtime", because the
    /// thread is already driving the executor. `block_in_place` hands the
    /// thread's other work to a sibling worker first, which is what makes the
    /// block legal.
    ///
    /// ```ignore
    /// let body = host.block_on(reqwest::get(url));
    /// ```
    ///
    /// The cost is a tokio worker thread for the duration of the call. That is
    /// the right trade for a handler that occasionally reaches the network; it
    /// is the wrong one if the *language* has async semantics of its own, where
    /// the interpreter would need to yield to a scheduler rather than block.
    pub fn block_on<F: std::future::Future>(&self, fut: F) -> F::Output {
        tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(fut))
    }
}
"#;

/// Kept in step with the workspace version.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Path to this checkout's `nh-runtime`.
///
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
    /// Syntactic flavour of the scaffolded language.
    pub style: Style,
    /// Optional capabilities: loops, functions.
    pub features: Features,
    /// Scaffold a bytecode compiler rather than an interpreter.
    ///
    /// The grammar, the generated code, and `eval_source` are the same either
    /// way — only `src/lib.rs` and the handlers differ, and only in that `Out`
    /// is `()` and bodies emit instead of compute.
    pub is_compiler: bool,
    /// Set up the project for async work in handlers.
    ///
    /// The evaluator stays synchronous — see `ASYNC_NOTE` for why, and for the
    /// one trap this exists to remove.
    pub is_async: bool,
}

impl Options {
    // Eight arguments, all of them independent answers `nh init` was given.
    // A builder or an options struct would only move the same eight somewhere
    // else, and this has exactly one caller.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        dir: PathBuf,
        name: Option<String>,
        ext: Option<String>,
        force: bool,
        is_async: bool,
        is_compiler: bool,
        style: Style,
        features: Features,
    ) -> Result<Self, String> {
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
            is_async,
            is_compiler,
            style,
            features,
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

/// The grammar template for this style.
fn grammar_template(style: Style) -> &'static str {
    match style {
        Style::C => GRAMMAR_C,
        Style::Basic => GRAMMAR_BASIC,
    }
}

fn render(template: &str, opts: &Options) -> String {
    let parts = opts.features.grammar_parts(opts.style);
    let arg_rule = if opts.features.has(Feature::Functions) {
        ARG_RULE
    } else {
        ""
    };
    let chunks = opts.features.host_chunks(opts.is_compiler);

    template
        .replace("{{reserved}}", &parts.reserved)
        .replace("{{stmt_loops}}", &parts.stmt_loops)
        .replace("{{stmt_functions}}", &parts.stmt_functions)
        .replace("{{rules_extra}}", &format!("{}{arg_rule}", parts.rules))
        .replace("{{primary_call}}", &parts.primary)
        .replace("{{name}}", &opts.name)
        .replace("{{Name}}", &opts.grammar)
        .replace("{{ext}}", &opts.ext)
        .replace("{{tokiodep}}", if opts.is_async { TOKIO_DEP } else { "" })
        .replace("{{tokiomain}}", if opts.is_async { "#[tokio::main(flavor = \"multi_thread\")]\n" } else { "" })
        .replace("{{mainasync}}", if opts.is_async { "async " } else { "" })
        .replace("{{asyncsupport}}", if opts.is_async { ASYNC_SUPPORT } else { "" })
        .replace(
            "{{sample_loops}}",
            match (opts.features.has(Feature::Loops), opts.style) {
                (false, _) => "",
                (true, Style::C) => SAMPLE_LOOPS_C,
                (true, Style::Basic) => SAMPLE_LOOPS_BASIC,
            },
        )
        .replace(
            "{{sample_functions}}",
            match (opts.features.has(Feature::Functions), opts.style) {
                (false, _) => "",
                (true, Style::C) => SAMPLE_FNS_C,
                (true, Style::Basic) => SAMPLE_FNS_BASIC,
            },
        )
        // How an identifier arrives, and how to turn one into a lookup. A
        // folding token binds as `&Name`, which keeps both spellings — see the
        // comment above `token IDENT` in the line-oriented grammar.
        .replace(
            "{{name_ty}}",
            match opts.style {
                Style::C => "&str",
                Style::Basic => "&Name",
            },
        )
        .replace(
            "{{name_import}}",
            match opts.style {
                Style::C => "",
                Style::Basic => "use nh_runtime::Name;\n",
            },
        )
        .replace(
            "{{key}}",
            match opts.style {
                Style::C => "",
                Style::Basic => ".key()",
            },
        )
        .replace("{{host_types}}", &chunks.types)
        .replace("{{host_state}}", &chunks.state)
        .replace("{{host_impl}}", &chunks.methods)
        .replace("{{vm_ops}}", &chunks.vm_ops)
        .replace("{{vm_exec}}", &chunks.vm_exec)
        .replace(
            "{{produced}}",
            if opts.is_compiler {
                PRODUCED_COMPILER
            } else {
                PRODUCED_INTERP
            },
        )

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
        (
            grammar_path.clone(),
            render(grammar_template(opts.style), opts),
        ),
        (opts.dir.join("Cargo.toml"), render(CARGO_TOML, opts)),
        (opts.dir.join("README.md"), render(README, opts)),
        (opts.dir.join(".gitignore"), render(GITIGNORE, opts)),
        (
            opts.dir.join(format!("sample.{}", opts.ext)),
            render(sample_template(opts.style), opts),
        ),
        (opts.dir.join("build.rs"), render(BUILD_RS, opts)),
        (
            src.join("lib.rs"),
            render(lib_template(opts), opts),
        ),
        (src.join("main.rs"), render(MAIN_RS, opts)),
    ];
    let handler_set = if opts.is_compiler {
        HANDLERS_COMPILER
    } else {
        HANDLERS
    };
    for name in crate::features::handler_names(opts.style, &opts.features) {
        let body = handler_set
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, b)| *b)
            .unwrap_or_else(|| panic!("no template for handler `{name}`"));
        files.push((handlers.join(format!("{name}.rs")), render(body, opts)));
    }

    let mut written = Vec::new();
    for (path, contents) in files {
        std::fs::write(&path, contents)
            .map_err(|e| format!("cannot write `{}`: {e}", path.display()))?;
        written.push(path);
    }

    // The runtime travels with the project rather than being fetched, so
    // `cargo run` needs no credentials and no cargo configuration.
    for rel in crate::vendor::write(&opts.dir, VERSION)
        .map_err(|e| format!("cannot vendor the runtime: {e}"))?
    {
        written.push(opts.dir.join(rel));
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

    /// A scaffolded project must depend on **pest and the vendored runtime**,
    /// and nothing else. Anything reached over the network is a credential, a
    /// cargo setting, or an outage between somebody and a working project.
    #[test]
    fn a_scaffold_depends_only_on_pest_and_the_vendored_runtime() {
        let toml = render(CARGO_TOML, &Options::new(
            PathBuf::from("/tmp/x"), Some("demo".into()), None, false, false, false, Style::C, Features::none(),
        ).unwrap());

        assert!(
            toml.contains(r#"nh-runtime = { path = "vendor/nh-runtime" }"#),
            "{toml}"
        );
        for absent in ["git =", "nh-build", "{{runtimedep}}", "{{builddep}}"] {
            assert!(!toml.contains(absent), "`{absent}` is still in:\n{toml}");
        }

        // `grammar-extras` is the setting that fails silently when missing.
        assert!(toml.contains(r#"features = ["grammar-extras"]"#), "{toml}");
    }

    /// `build.rs` calls the binary rather than linking the generator, which is
    /// what keeps the dependency list to pest.
    #[test]
    fn the_build_script_shells_out_rather_than_depending_on_the_generator() {
        let build = render(BUILD_RS, &Options::new(
            PathBuf::from("/tmp/x"), Some("demo".into()), None, false, false, false, Style::C, Features::none(),
        ).unwrap());

        assert!(build.contains("Command::new"), "{build}");
        assert!(!build.contains("nh_build::"), "{build}");
        // A missing binary must say what to do about it.
        assert!(build.contains("cargo install"), "{build}");
    }

    #[test]
    fn a_keyword_name_is_refused() {
        // `nh init pub` would generate `use pub::...`, which does not compile.
        let err = Options::new(
            PathBuf::from("/tmp/pub"), Some("pub".into()), None, false, false, false,
            Style::C, Features::none(),
        )
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
        let opts = Options::new(
            PathBuf::from("/tmp/x"), Some("demo".into()), None, false, false, false,
            Style::C, Features::all(),
        ).unwrap();
        for t in [GRAMMAR_C, GRAMMAR_BASIC, CARGO_TOML, MAIN_RS, README, GITIGNORE, SAMPLE_C, SAMPLE_BASIC] {
            let out = render(t, &opts);
            assert!(
                !out.contains("{{"),
                "unreplaced placeholder in template:\n{out}"
            );
        }
    }
}
