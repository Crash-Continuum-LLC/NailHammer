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

    let _ = writeln!(
        out,
        "{doc}\
         {arity}\
         pub fn run<H: Handlers>(host: &mut H{sig}, cx: &mut Ctx) -> Result<H::Out> {{\n\
        \x20   compile_error!(\n\
        \x20       \"handler `{}` is not implemented. Delete this line, then return \\\n\
        \x20        a value built from the parameters above.\"\n\
        \x20   );\n\n\
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
pub fn generate_mod(lowered: &Lowered) -> String {
    let mut out = String::new();
    out.push_str(HEADER);
    let _ = writeln!(
        out,
        "\n// One module per labelled alternative. Each is a small file you own;\n\
         // this list is regenerated so a new alternative wires itself up.\n"
    );

    let mut names: Vec<&str> = lowered
        .alternatives
        .iter()
        .map(|a| a.pest_rule.as_str())
        .collect();
    names.sort_unstable();

    for name in names {
        let _ = writeln!(out, "pub mod {name};");
    }

    out
}
