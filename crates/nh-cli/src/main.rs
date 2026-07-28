//! The `nh` command line tool.
//!
//! At M1 `check`, `build`, and `explain` all do real work. `build` emits the
//! generated `.pest`; the Rust side — views, handler dispatch, the operator
//! driver — is M2 and M3.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use nh_syntax::{render, resolve, Ast, Errors, SourceMap};

mod init;
mod json;

const USAGE: &str = "\
nh — NailHammer grammar toolkit

USAGE:
    nh init    [dir] [--name <name>] [--ext <ext>] [--force]
    nh check   <file.nh> [--quiet]
    nh build   <file.nh> [-o <out.pest>] [--rust <src-dir>] [--prune [--force]]
    nh explain <file.nh> [--source]
    nh --help | --version

COMMANDS:
    init       Scaffold a runnable language project
    check      Parse a grammar, resolve its imports, and print the merged result
    build      Generate the .pest grammar
    explain    Show the resolved operator table

OPTIONS:
    --name     init: project name (default: the directory name)
    --ext      init: source file extension for your language (default: the name)
    --force    init: write into a non-empty directory
               build: with --prune, remove implemented handlers too
    --quiet    check: report diagnostics only
    --deny-warnings
               check: treat analysis warnings as errors, for CI
    --lints    check: list the determinism lints and exit
    -o <path>  build: write the .pest here instead of alongside the .nh file
    --rust <d> build: also generate views, dispatch, and handler stubs into <d>
    --prune    build: remove handler files with no matching grammar alternative
    --source   explain: print the table as .nh source you could paste

New here? `nh init mylang && cd mylang && cargo run`

Not yet implemented:
    Operator driver — folding expressions by precedence   (milestone M3)
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.is_empty() || args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("nh {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }

    match args[0].as_str() {
        "init" => init_cmd(&args[1..]),
        "check" => check(&args[1..]),
        "build" => build(&args[1..]),
        "explain" => explain(&args[1..]),
        other => {
            eprintln!("error: unknown command `{other}`\n");
            eprint!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

/// Parses flags, returning the single positional file argument.
fn parse_args(args: &[String], flags: &[&str], valued: &[&str]) -> Result<Parsed, String> {
    let mut out = Parsed::default();
    let mut it = args.iter();

    while let Some(arg) = it.next() {
        if valued.contains(&arg.as_str()) {
            match it.next() {
                Some(v) => out.values.push((arg.clone(), v.clone())),
                None => return Err(format!("`{arg}` needs a value")),
            }
        } else if flags.contains(&arg.as_str()) {
            out.flags.push(arg.clone());
        } else if arg.starts_with('-') {
            return Err(format!("unknown option `{arg}`"));
        } else if out.path.is_none() {
            out.path = Some(PathBuf::from(arg));
        } else {
            return Err(format!("unexpected argument `{arg}`"));
        }
    }

    Ok(out)
}

#[derive(Default)]
struct Parsed {
    path: Option<PathBuf>,
    flags: Vec<String>,
    values: Vec<(String, String)>,
}

impl Parsed {
    fn has(&self, flag: &str) -> bool {
        self.flags.iter().any(|f| f == flag)
    }
    fn value(&self, key: &str) -> Option<&str> {
        self.values
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

fn usage_error(msg: String) -> ExitCode {
    eprintln!("error: {msg}\n");
    eprint!("{USAGE}");
    ExitCode::from(2)
}

/// Loads a grammar and resolves its operator table, reporting through one path
/// so every subcommand's diagnostics look identical.
fn load(path: &Path, sm: &mut SourceMap) -> Result<(Ast, nh_operators::OperatorTable), Errors> {
    let ast = resolve(sm, path)?;
    let table = nh_operators::resolve(&ast, sm)?;
    Ok((ast, table))
}

fn report(errors: &Errors, sm: &SourceMap) -> ExitCode {
    eprint!("{}", errors.render(sm));
    let n = errors.0.len();
    eprintln!("{n} error{} emitted", if n == 1 { "" } else { "s" });
    ExitCode::FAILURE
}

// ---------------------------------------------------------------------------

fn init_cmd(args: &[String]) -> ExitCode {
    let parsed = match parse_args(args, &["--force"], &["--name", "--ext"]) {
        Ok(p) => p,
        Err(e) => return usage_error(e),
    };

    let dir = parsed.path.clone().unwrap_or_else(|| PathBuf::from("."));
    let opts = match init::Options::new(
        dir,
        parsed.value("--name").map(str::to_string),
        parsed.value("--ext").map(str::to_string),
        parsed.has("--force"),
    ) {
        Ok(o) => o,
        Err(e) => return usage_error(e),
    };

    let created = match init::scaffold(&opts) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    // Generate the .pest through the real pipeline rather than shipping a
    // pre-baked one, so a scaffolded project is proven to build by the same
    // code path the user will run.
    let mut sm = SourceMap::new();
    let (ast, table) = match load(&created.grammar_path, &mut sm) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: the scaffolded grammar did not check — this is a bug in nh init");
            return report(&e, &sm);
        }
    };
    let lowered = match nh_lower::lower(&ast, &table) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("error: the scaffolded grammar did not lower — this is a bug in nh init");
            return report(&e, &sm);
        }
    };
    if let Err(e) = std::fs::write(&created.pest_path, &lowered.pest) {
        eprintln!("error: cannot write `{}`: {e}", created.pest_path.display());
        return ExitCode::FAILURE;
    }

    // Generate the Rust side too. The scaffold ships working handlers, and
    // `--rust` never overwrites an existing handler file, so this fills in the
    // generated half without touching them.
    let src_dir = created.pest_path.parent().unwrap_or(&opts.dir).to_path_buf();
    if emit_rust(&src_dir, &ast, &table, &lowered, false, false) != ExitCode::SUCCESS {
        eprintln!("error: scaffolded codegen failed — this is a bug in nh init");
        return ExitCode::FAILURE;
    }

    let root = opts.dir.display().to_string();
    println!("Created {} project `{}`", opts.grammar, opts.name);
    for f in created.files.iter().chain(std::iter::once(&created.pest_path)) {
        println!("  {}", f.display());
    }
    println!(
        "\nNext:\n  cd {root}\n  cargo run\n\nThen edit {name}.nh and re-run:\n  \
         nh build {name}.nh -o src/{name}.pest --rust src",
        name = opts.name
    );

    ExitCode::SUCCESS
}

fn check(args: &[String]) -> ExitCode {
    let parsed = match parse_args(args, &["--quiet", "-q", "--deny-warnings", "--lints", "--json"], &[]) {
        Ok(p) => p,
        Err(e) => return usage_error(e),
    };
    if parsed.has("--lints") {
        println!("Determinism lints. Silence one with `allow <name> in <rule>;`\n");
        let width = nh_analysis::LINTS.iter().map(|(n, _)| n.len()).max().unwrap_or(0);
        for (name, description) in nh_analysis::LINTS {
            println!("  {name:<width$}  {description}");
        }
        return ExitCode::SUCCESS;
    }

    let Some(path) = parsed.path.clone() else {
        return usage_error("`nh check` needs a file".into());
    };

    let mut sm = SourceMap::new();
    let (ast, table) = match load(&path, &mut sm) {
        Ok(v) => v,
        Err(e) => return report(&e, &sm),
    };

    // Lowering is where undefined references and unguardable tokens surface, so
    // `check` runs it rather than reporting a clean grammar that `build` would
    // then reject. Its warnings are surfaced too — they were computed and
    // dropped before.
    let lowering = match nh_lower::lower(&ast, &table) {
        Ok(l) => l,
        Err(e) => return report(&e, &sm),
    };

    // Determinism analysis. Warnings are printed either way; `--deny-warnings`
    // makes them fatal so CI can hold the line.
    let mut diagnostics = lowering.diagnostics.clone();
    diagnostics.extend(nh_analysis::analyse(&ast, table.atom_rule.as_deref()));
    let errors = diagnostics
        .iter()
        .filter(|d| d.severity == nh_syntax::Severity::Error)
        .count();
    let warnings = diagnostics.len() - errors;

    if parsed.has("--json") {
        // Only the array reaches stdout, so a caller need not strip anything.
        println!("{}", json::diagnostics(&sm, &diagnostics));
        return if errors > 0 {
            ExitCode::FAILURE
        } else {
            ExitCode::SUCCESS
        };
    }

    for d in &diagnostics {
        eprint!("{}", d.render(&sm));
        eprintln!();
    }

    if errors > 0 || (warnings > 0 && parsed.has("--deny-warnings")) {
        eprintln!(
            "{errors} error(s), {warnings} warning(s){}",
            if errors == 0 { " (denied)" } else { "" }
        );
        return ExitCode::FAILURE;
    }

    if !parsed.has("--quiet") && !parsed.has("-q") {
        print!("{}", render(&ast));
    }

    eprintln!(
        "ok: {} checked  [{} rule(s), {} token(s), {} operator(s){}]",
        path.display(),
        ast.rules.len(),
        ast.tokens.len(),
        table.operators().count(),
        if warnings > 0 {
            format!(", {warnings} warning(s)")
        } else {
            String::new()
        }
    );
    ExitCode::SUCCESS
}

fn build(args: &[String]) -> ExitCode {
    let parsed = match parse_args(args, &["--prune", "--force"], &["-o", "--output", "--rust"]) {
        Ok(p) => p,
        Err(e) => return usage_error(e),
    };
    let Some(path) = parsed.path.clone() else {
        return usage_error("`nh build` needs a file".into());
    };

    let mut sm = SourceMap::new();
    let (ast, table) = match load(&path, &mut sm) {
        Ok(v) => v,
        Err(e) => return report(&e, &sm),
    };

    let lowered = match nh_lower::lower(&ast, &table) {
        Ok(l) => l,
        Err(e) => return report(&e, &sm),
    };

    let out = parsed
        .value("-o")
        .or_else(|| parsed.value("--output"))
        .map(PathBuf::from)
        .unwrap_or_else(|| path.with_extension("pest"));

    // Create the directory rather than failing: `-o src/x.pest` in a fresh
    // project is the obvious first command, and `src/` may not exist yet.
    if let Some(parent) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
        if let Err(e) = std::fs::create_dir_all(parent) {
            eprintln!("error: cannot create `{}`: {e}", parent.display());
            return ExitCode::FAILURE;
        }
    }
    if let Err(e) = std::fs::write(&out, &lowered.pest) {
        eprintln!("error: cannot write `{}`: {e}", out.display());
        return ExitCode::FAILURE;
    }

    for d in &lowered.diagnostics {
        eprint!("{}", d.render(&sm));
        eprintln!();
    }
    eprintln!("ok: wrote {}", out.display());

    if let Some(rust_dir) = parsed.value("--rust") {
        return emit_rust(
            Path::new(rust_dir),
            &ast,
            &table,
            &lowered,
            parsed.has("--prune"),
            parsed.has("--force"),
        );
    }

    if parsed.has("--prune") {
        eprintln!("error: `--prune` only applies with `--rust <src-dir>`");
        return ExitCode::from(2);
    }

    eprintln!(
        "note: {} labelled alternative(s); pass --rust <src-dir> to generate handlers",
        lowered.alternatives.len()
    );
    ExitCode::SUCCESS
}

/// Writes the generated Rust, honouring DESIGN.md §5.4's regeneration policy:
/// generated files are always overwritten, handler stubs are written once and
/// never overwritten or deleted.
fn emit_rust(
    dir: &Path,
    ast: &Ast,
    table: &nh_operators::OperatorTable,
    lowered: &nh_lower::Lowered,
    prune: bool,
    force: bool,
) -> ExitCode {
    let opts = nh_codegen::Options::default();
    let generated = nh_codegen::generate(ast, table, lowered, &opts);

    let (mut written, mut kept, mut created) = (0usize, 0usize, 0usize);

    for file in &generated.files {
        let path = dir.join(&file.path);
        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                eprintln!("error: cannot create `{}`: {e}", parent.display());
                return ExitCode::FAILURE;
            }
        }

        match file.policy {
            nh_codegen::Policy::Generated => {
                if let Err(e) = std::fs::write(&path, &file.contents) {
                    eprintln!("error: cannot write `{}`: {e}", path.display());
                    return ExitCode::FAILURE;
                }
                written += 1;
            }
            nh_codegen::Policy::OnceOnly => {
                if path.exists() {
                    kept += 1;
                } else {
                    if let Err(e) = std::fs::write(&path, &file.contents) {
                        eprintln!("error: cannot write `{}`: {e}", path.display());
                        return ExitCode::FAILURE;
                    }
                    created += 1;
                }
            }
        }
    }

    eprintln!(
        "ok: generated {} file(s) in {}  [{created} new handler(s), {kept} kept]",
        written + created,
        dir.display()
    );

    if let Err(e) = prune_orphans(dir, &generated.handler_modules, prune, force) {
        eprintln!("error: {e}");
        return ExitCode::FAILURE;
    }

    // Renaming or reordering a binding changes nothing the compiler can see,
    // because parameters are positional. Reordering two of the same type is a
    // silent defect, so it is reported here.
    let drift = nh_codegen::drift::check_all(lowered, |rel| {
        std::fs::read_to_string(dir.join(rel)).ok()
    });
    let mut fatal = false;
    for (alt, d) in &drift {
        let rel = format!("handlers/{}.rs", alt.pest_rule);
        if d.is_error() {
            fatal = true;
            eprintln!("error: {}", d.message(&rel));
        } else {
            eprintln!("warning: {}", d.message(&rel));
        }
    }

    if fatal {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// A handler file with no matching alternative in the grammar.
struct Orphan {
    path: PathBuf,
    name: String,
    /// Whether it contains work, as opposed to being an untouched stub.
    ///
    /// A generated stub carries a `compile_error!` telling you to delete that
    /// line. Its absence means somebody did — so the file holds real code, and
    /// removing it without asking would destroy work.
    implemented: bool,
}

fn find_orphans(dir: &Path, expected: &[String]) -> Vec<Orphan> {
    let handlers = dir.join("handlers");
    let Ok(entries) = std::fs::read_dir(&handlers) else {
        return Vec::new();
    };

    let mut orphans: Vec<Orphan> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            let stem = name.strip_suffix(".rs")?.to_string();
            if stem == "mod" || expected.contains(&stem) {
                return None;
            }
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            Some(Orphan {
                implemented: !text.contains(nh_codegen::stubs::UNIMPLEMENTED_MARKER),
                path,
                name: stem,
            })
        })
        .collect();
    orphans.sort_by(|a, b| a.name.cmp(&b.name));
    orphans
}

/// Reports orphaned handlers, and removes them when asked.
///
/// DESIGN.md §5.4 is explicit that generated files are overwritten and handler
/// files are not, so deletion is never automatic. `--prune` removes only
/// handlers that were **never implemented**; one containing real code needs
/// `--force` as well, because "this rule no longer exists" is not the same
/// claim as "you do not want this code".
fn prune_orphans(dir: &Path, expected: &[String], prune: bool, force: bool) -> Result<(), String> {
    let orphans = find_orphans(dir, expected);
    if orphans.is_empty() {
        return Ok(());
    }

    let (mut removed, mut kept) = (0usize, Vec::new());

    for orphan in &orphans {
        let removable = prune && (!orphan.implemented || force);
        if removable {
            std::fs::remove_file(&orphan.path)
                .map_err(|e| format!("cannot remove `{}`: {e}", orphan.path.display()))?;
            eprintln!("removed handlers/{}.rs", orphan.name);
            removed += 1;
        } else {
            kept.push(orphan);
        }
    }

    if !kept.is_empty() {
        eprintln!(
            "warning: {} handler file(s) no longer match any grammar alternative:",
            kept.len()
        );
        for orphan in &kept {
            let note = if orphan.implemented {
                "implemented — contains your code"
            } else {
                "never implemented"
            };
            eprintln!("  handlers/{}.rs  ({note})", orphan.name);
        }

        if !prune {
            eprintln!("note: pass --prune to remove the unimplemented ones");
        }
        if kept.iter().any(|o| o.implemented) {
            eprintln!(
                "note: pass --prune --force to remove implemented ones too, but read them first"
            );
        }
    }

    if removed > 0 {
        eprintln!("ok: removed {removed} orphaned handler(s)");
    }
    Ok(())
}

fn explain(args: &[String]) -> ExitCode {
    let parsed = match parse_args(args, &["--source"], &[]) {
        Ok(p) => p,
        Err(e) => return usage_error(e),
    };
    let Some(path) = parsed.path.clone() else {
        return usage_error("`nh explain` needs a file".into());
    };

    let mut sm = SourceMap::new();
    let (ast, table) = match load(&path, &mut sm) {
        Ok(v) => v,
        Err(e) => return report(&e, &sm),
    };

    if parsed.has("--source") {
        // Presets have no privileged status (DESIGN.md §6.1): print the table
        // as `.nh` the user could have written themselves.
        match ast.uses.first().and_then(|u| {
            nh_operators::presets::source(&u.preset.value).map(|s| (u.preset.value.clone(), s))
        }) {
            Some((name, src)) => println!("// operators::{name}\n{}", src.trim()),
            None => println!("// this grammar's table is written in the grammar itself"),
        }
        return ExitCode::SUCCESS;
    }

    print!("{}", nh_operators::explain(&table));
    ExitCode::SUCCESS
}
