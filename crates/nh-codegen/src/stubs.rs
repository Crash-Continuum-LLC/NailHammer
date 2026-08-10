//! Handler stub generation.
//!
//! One file per labelled alternative, **written once and never overwritten**
//! (DESIGN.md §5.4). The stub's job is to be immediately obvious: it shows the
//! grammar alternative it came from and lists the accessors the view offers, so
//! the first thing you see is what you have to work with.

use nh_lower::{Lowered, LoweredAlternative};

use crate::params::params;
use std::fmt::Write as _;

use crate::{ident, Options, HEADER};

/// The sentence a generated stub carries until someone implements it.
///
/// `--prune` uses this to tell an orphan that was never written from one that
/// contains real work: the stub instructs you to delete the line, so its
/// absence means somebody did.
pub const UNIMPLEMENTED_MARKER: &str = "is not implemented. Delete this line";

pub fn generate(alt: &LoweredAlternative, opts: &Options) -> String {
    let ps = params(alt);
    let mut out = String::new();

    let _ = writeln!(
        out,
        "//! Handler for `{}`.\n\
         //!\n\
         //! From this alternative of `rule {}`:\n\
         //!\n\
         //! ```text\n\
         //! {}\n\
         //! ```\n\
         //!\n\
         //! Created once by `nh build --rust` and never overwritten. Edit freely.\n",
        alt.pest_rule, alt.rule, alt.source
    );

    // Import only what the signature actually mentions, in the usual order:
    // std, then the runtime, then this crate. A `lazy` parameter is an `Shared` of
    // a generated AST type, so both have to come along.
    let mut lazy: Vec<&str> = ps
        .iter()
        .filter_map(|p| p.ty.split("Shared<").nth(1))
        .filter_map(|rest| rest.split('>').next())
        .collect();
    lazy.sort_unstable();
    lazy.dedup();

    let std_import = if lazy.is_empty() {
        String::new()
    } else {
        "use nh_runtime::Shared;\n\n".to_string()
    };
    let ast_import = match lazy.len() {
        0 => String::new(),
        1 => format!("use {}::ast::{};\n", opts.module_path, lazy[0]),
        _ => format!("use {}::ast::{{{}}};\n", opts.module_path, lazy.join(", ")),
    };
    let name_import = if ps.iter().any(|p| p.ty.contains("Name")) {
        "use nh_runtime::Name;\n"
    } else {
        ""
    };
    // A project with a VM target compiles rather than evaluates, so its
    // handlers call `emit`, `alloc` and the rest. Importing the trait here
    // means the first line an author writes in a stub compiles, instead of
    // failing with `no method named alloc` and a note about traits in scope.
    let emitter = if opts.target.is_some() {
        "\nuse nh_vm::Emitter;"
    } else {
        ""
    };
    let dispatch_items = if lazy.is_empty() {
        "Handlers".to_string()
    } else {
        "{Eval, Handlers}".to_string()
    };

    let _ = writeln!(
        out,
        "{std_import}\
         use nh_runtime::{{Ctx, Result}};{emitter}\n\
         {name_import}\n\
         {ast_import}\
         use {}::dispatch::{dispatch_items};\n",
        opts.module_path
    );

    // The parameters *are* the inputs. No view, no traversal, no fetching.
    let doc: String = ps
        .iter()
        .map(|p| format!("/// * `{}` — {}\n", p.name, p.doc))
        .collect();
    let sig: String = ps
        .iter()
        .map(|p| format!(", {}: {}", ident(&p.name), stub_ty(&p.ty)))
        .collect();

    // `host` and `cx` push a six-binding rule over clippy's limit. The user
    // owns this file but did not choose its parameter list — the grammar did —
    // so the attribute ships with the stub rather than as a surprise later.
    let arity = if ps.len() + 2 > 7 {
        "#[allow(clippy::too_many_arguments)]\n"
    } else {
        ""
    };

    // A stub reports at *run time* rather than refusing to compile.
    //
    // This used to be a `compile_error!`, which meant a project could not be
    // built at all until every last handler was written — no `cargo build`, no
    // test, no running the parts already finished. Worse, `nh_handlers!` cannot
    // expand while any stub is a `compile_error!`, so `Handlers` does not exist,
    // so `.eval(..)` does not resolve — and `lazy` handlers, the ones for
    // conditionals, loops and access chains, were exactly the ones that could
    // not be written until everything else already was.
    //
    // Nothing is lost by moving it: `cx.err` reports at this node, with a span,
    // naming the handler. And `nh build` counts what is still a stub, so the
    // number stays in front of you without holding the build hostage.
    let _ = writeln!(
        out,
        "{doc}\
         {arity}\
         #[allow(unused_variables)]\n\
         pub fn run<H: Handlers>(host: &mut H{sig}, cx: &mut Ctx) -> Result<H::Out> {{\n\
        \x20   // handler `{}` is not implemented. Delete this line, then return\n\
        \x20   // a value built from the parameters above.\n\
        \x20   //\n\
        \x20   // `cx.err(..)` reports at this node — no span threading needed.\n\
        \x20   cx.err(\"`{}` is not implemented yet\")\n\
         }}",
        alt.pest_rule, alt.pest_rule
    );

    out
}

/// A stub is generic over `H`, so `Self::Out` is spelled `H::Out`.
fn stub_ty(ty: &str) -> String {
    ty.replace("Self::Out", "H::Out")
}

/// `handlers/mod.rs` — always regenerated, since it only lists modules.
/// Where a handler lives, relative to the Rust output root.
///
/// # Why not one flat directory
///
/// A grammar's rules group its alternatives, and the handlers should say so.
/// Crash BASIC has a hundred of them, forty-nine belonging to `stmt` — flat,
/// that is a directory nobody can read, and the grouping the grammar already
/// states is thrown away on the way out.
///
/// So an alternative of a multi-alternative rule lives under its rule:
/// `handlers/stmt/assign.rs`. A rule with one alternative has nothing to group
/// and stays where it is: `handlers/program.rs`.
pub fn handler_path(alt: &LoweredAlternative) -> String {
    match segments(alt) {
        (rule, None) => format!("handlers/{rule}.rs"),
        (rule, Some(label)) => format!("handlers/{rule}/{label}.rs"),
    }
}

/// The Rust module path, escaped — a label may be a keyword (`for`, `type`).
pub fn handler_module(alt: &LoweredAlternative) -> String {
    match segments(alt) {
        (rule, None) => crate::ident(rule),
        (rule, Some(label)) => format!("{}::{}", crate::ident(rule), crate::ident(label)),
    }
}

/// The rule, and the label when the rule has more than one alternative.
///
/// `pest_rule` is the rule's own name when there is nothing to disambiguate,
/// which is exactly when there is nothing to group either.
fn segments(alt: &LoweredAlternative) -> (&str, Option<&str>) {
    if alt.pest_rule == alt.rule {
        (&alt.rule, None)
    } else {
        (&alt.rule, Some(&alt.label))
    }
}

pub fn generate_mod(lowered: &Lowered) -> String {
    let mut out = String::new();
    out.push_str(HEADER);
    let _ = writeln!(
        out,
        "\n// One module per labelled alternative. Each is a small file you own;\n\
         // this list is regenerated so a new alternative wires itself up.\n"
    );

    let mut names: Vec<String> = lowered
        .alternatives
        .iter()
        .map(|a| crate::ident(segments(a).0))
        .collect();
    names.sort();
    names.dedup();

    for name in names {
        let _ = writeln!(out, "pub mod {name};");
    }

    out
}

/// `handlers/<rule>/mod.rs` — one per rule that has alternatives to group.
///
/// Returns `(path, contents)` for each. A rule with a single alternative has no
/// directory and no module file.
pub fn generate_rule_mods(lowered: &Lowered) -> Vec<(String, String)> {
    let mut by_rule: std::collections::BTreeMap<&str, Vec<&str>> = Default::default();
    for alt in &lowered.alternatives {
        if let (rule, Some(label)) = segments(alt) {
            by_rule.entry(rule).or_default().push(label);
        }
    }

    by_rule
        .into_iter()
        .map(|(rule, mut labels)| {
            labels.sort_unstable();
            labels.dedup();
            let mut out = String::new();
            out.push_str(HEADER);
            let _ = writeln!(
                out,
                "\n// One module per alternative of `rule {rule}`. Each is a small\n\
                 // file you own; this list is regenerated so a new alternative\n\
                 // wires itself up.\n"
            );
            for l in labels {
                let _ = writeln!(out, "pub mod {};", crate::ident(l));
            }
            (format!("handlers/{rule}/mod.rs"), out)
        })
        .collect()
}

/// Moves handlers written under the old flat layout into their rule's
/// directory.
///
/// Handler files are never overwritten and never deleted, so changing where
/// they live cannot mean abandoning them: a project that regenerates would
/// otherwise get a hundred fresh stubs and a hundred orphan reports, with its
/// actual work in the orphans. Same file, same contents, new path — done once,
/// and said out loud because moving somebody's code silently is worse than the
/// sprawl it fixes.
pub fn migrate_flat_handlers(dir: &std::path::Path, lowered: &Lowered) -> Result<usize, String> {
    let mut moved = 0;
    for alt in &lowered.alternatives {
        let want = handler_path(alt);
        let flat = format!("handlers/{}.rs", alt.pest_rule);
        if want == flat {
            continue;
        }
        let (from, to) = (dir.join(&flat), dir.join(&want));
        if !from.is_file() {
            continue;
        }
        // A destination already there is usually somebody's work and must not
        // be touched. An untouched *stub* is not: it is what a build produced
        // before the flat file was noticed, and leaving it would strand the
        // real handler beside it as an orphan.
        if to.exists() {
            let existing = std::fs::read_to_string(&to).unwrap_or_default();
            let arriving = std::fs::read_to_string(&from).unwrap_or_default();
            let replacing_a_stub = existing.contains(UNIMPLEMENTED_MARKER)
                && !arriving.contains(UNIMPLEMENTED_MARKER);
            if !replacing_a_stub {
                continue;
            }
            std::fs::remove_file(&to).map_err(|e| format!("{want}: {e}"))?;
        }
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::rename(&from, &to).map_err(|e| format!("{flat}: {e}"))?;
        eprintln!("moved {flat} -> {want}");
        moved += 1;
    }
    Ok(moved)
}
