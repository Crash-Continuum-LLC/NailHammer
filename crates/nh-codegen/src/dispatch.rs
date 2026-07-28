//! The trait stack and the evaluator.
//!
//! DESIGN.md §5.4's answer to "giant unmaintainable files": `Handlers` declares
//! one **required** method per labelled alternative, and the generated
//! `nh_handlers!` macro writes an impl whose bodies do nothing but delegate to
//! a per-alternative module. You write one small file each; Rust's own trait
//! exhaustiveness means adding an alternative to the `.nh` breaks the build
//! until a handler exists. No runtime registry that can silently miss.
//!
//! Since M7 the evaluator walks the **owned** AST rather than pest pairs. The
//! shape is unchanged — evaluate the children, then call the handler — but
//! nothing it hands over borrows the parse, so a `lazy` parameter can be kept
//! and re-run (DESIGN.md §9).

use nh_lower::{Lowered, LoweredAlternative, LoweredRule, LoweredVariant, RuleShape};
use nh_operators::OperatorTable;
use std::collections::HashMap;
use std::fmt::Write as _;

use crate::operators::{emit_discriminants, emit_driver, emit_trait, emit_value_operators, emitted};
use crate::params::{params, Param};
use crate::{ident, type_name, Options, HEADER};

pub fn generate(lowered: &Lowered, table: &OperatorTable, opts: &Options) -> String {
    let mut out = String::new();
    out.push_str(HEADER);

    let ops = emitted(table);

    // The driver evaluates folded expressions. A grammar with no operator table
    // has none, and importing its runtime would be an unused import in a file
    // the reader cannot edit (DESIGN.md §11, standing constraint 6).
    let ops_import = if lowered.has_expr {
        "use nh_runtime::ops::{Assoc, Fixity, OpInfo};\n"
    } else {
        ""
    };

    // `Place` only exists as a resolvable thing when there is an operator
    // table for assignment to live in.
    let place_import = if lowered.has_expr {
        "use super::place::{resolve_place, Place};\n"
    } else {
        ""
    };

    let _ = writeln!(
        out,
        "\n#![allow(dead_code, unused_imports, unused_variables)]\n\
         // A rule with many bindings makes a method with many parameters. That\n\
         // is the grammar's shape, not a defect, and not something a reader of\n\
         // this file could act on.\n\
         #![allow(clippy::too_many_arguments)]\n\n\
         use std::rc::Rc;\n\n\
         use nh_runtime::{{Ctx, Diagnostic, Error, Name, Result, Span}};\n\
         {ops_import}\n\
         use super::ast::*;\n\
         {place_import}\
         use crate::Rule;\n"
    );

    emit_semantics(&mut out);
    emit_discriminants(&mut out, &ops);
    emit_trait(&mut out, &ops);
    emit_handlers(&mut out, lowered);
    emit_eval(&mut out, lowered);
    if lowered.has_expr {
        // Which evaluator an atom goes to. `rule atom = primary;` means the
        // atoms in the stream are `primary` nodes, so the alias is resolved
        // here rather than emitting an evaluator that does nothing.
        let atom = lowered
            .rules
            .iter()
            .find(|r| r.name == "atom")
            .map(|r| match &r.shape {
                RuleShape::Alias { child: Some(c) } => ident(c),
                _ => ident(&r.name),
            })
            .unwrap_or_else(|| "atom".to_string());
        emit_driver(&mut out, &ops, &atom);
        emit_value_operators(&mut out, &ops);
    }
    emit_macro(&mut out, lowered, opts);

    out
}

fn emit_semantics(out: &mut String) {
    let _ = writeln!(
        out,
        "\n// ---------------------------------------------------------------------------\n\
         // The trait stack\n\
         //\n\
         // One associated `Out` flows through all three traits, so an interpreter, a\n\
         // bytecode emitter, and a typechecker are three impls over one grammar.\n\
         // ---------------------------------------------------------------------------\n\n\
         pub trait Semantics {{\n\
        \x20   /// What evaluating a node produces.\n\
        \x20   type Out;\n\
         }}\n\n\
         /// A host whose `Out` is a **value it can ask questions about**.\n\
         ///\n\
         /// An interpreter implements this: `Out` is a value, and truthiness is\n\
         /// a question it can answer. A compiler does not: its `Out` is a\n\
         /// placeholder for something the *target machine* will compute later,\n\
         /// so there is nothing to inspect at build time.\n\
         ///\n\
         /// It is separate from [`Semantics`] for exactly that reason. Requiring\n\
         /// it of every host would force a bytecode emitter to write a `truthy`\n\
         /// it can never answer and must never be asked.\n\
         pub trait Values: Semantics {{\n\
        \x20   /// Used by the short-circuit operator bodies in\n\
        \x20   /// `nh_value_operators!`.\n\
        \x20   fn truthy(&self, value: &Self::Out) -> bool;\n\n\
        \x20   /// Used by the `??` body. Languages without a null need not\n\
        \x20   /// override it.\n\
        \x20   fn is_null(&self, value: &Self::Out) -> bool {{\n\
        \x20       let _ = value;\n\
        \x20       false\n\
        \x20   }}\n\
         }}\n"
    );
}

fn emit_handlers(out: &mut String, lowered: &Lowered) {
    let _ = writeln!(
        out,
        "/// One **required** method per labelled alternative.\n\
         ///\n\
         /// Each takes the alternative's bindings, already evaluated — the\n\
         /// generated evaluator does the walking. Nothing is defaulted: adding\n\
         /// an alternative to the grammar must break the build until a handler\n\
         /// exists.\n\
         pub trait Handlers: Operators + Sized {{"
    );

    for alt in &lowered.alternatives {
        let m = ident(&alt.pest_rule);
        let ps = params(alt);
        let sig = Param::signature(&ps);
        let doc = Param::doc_lines(&ps);

        let _ = writeln!(
            out,
            "\n    /// `{}` — from `rule {} = {}`.\n{doc}\
            \x20   fn {m}(&mut self{sig}, cx: &mut Ctx) -> Result<Self::Out>;",
            alt.pest_rule, alt.rule, alt.source
        );
    }

    let _ = writeln!(out, "}}\n");
}

// ---------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------

fn emit_eval(out: &mut String, lowered: &Lowered) {
    let by_alt: HashMap<&str, &LoweredAlternative> = lowered
        .alternatives
        .iter()
        .map(|a| (a.pest_rule.as_str(), a))
        .collect();

    let _ = writeln!(
        out,
        "// ---------------------------------------------------------------------------\n\
         // Evaluation\n\
         // ---------------------------------------------------------------------------\n\n\
         /// Runs a node.\n\
         ///\n\
         /// Implemented for every generated AST type, so a `lazy` parameter is\n\
         /// run with `.eval(host, cx)?` at whatever moment the handler chooses —\n\
         /// including never, or more than once.\n\
         pub trait Eval {{\n\
        \x20   fn eval<H: Handlers>(&self, host: &mut H, cx: &mut Ctx) -> Result<H::Out>;\n\
         }}\n"
    );

    for rule in &lowered.rules {
        match &rule.shape {
            // An alias carries no node, so it needs no evaluator: callers
            // resolve through it to whatever does.
            RuleShape::Alias { .. } => {}
            RuleShape::Single { pest_rule } => {
                if let Some(alt) = by_alt.get(pest_rule.as_str()) {
                    emit_eval_struct(out, &type_name(&rule.name), &ident(&rule.name), alt);
                }
            }
            RuleShape::Choice(variants) => {
                emit_eval_choice(out, rule, variants);
                for v in variants {
                    if let LoweredVariant::Labelled { pest_rule, .. } = v {
                        if let Some(alt) = by_alt.get(pest_rule.as_str()) {
                            emit_eval_struct(out, &type_name(pest_rule), &ident(pest_rule), alt);
                        }
                    }
                }
            }
        }
    }
}

fn emit_eval_choice(out: &mut String, rule: &LoweredRule, variants: &[LoweredVariant]) {
    let ty = type_name(&rule.name);
    let f = ident(&rule.name);

    let _ = writeln!(
        out,
        "/// Evaluates a `{}` by handing off to whichever alternative matched.\n\
         pub fn eval_{f}<H: Handlers>(host: &mut H, node: &{ty}, cx: &mut Ctx) -> Result<H::Out> {{\n\
        \x20   match node {{",
        rule.name
    );

    for v in variants {
        match v {
            LoweredVariant::Labelled { label, pest_rule } => {
                let _ = writeln!(
                    out,
                    "        {ty}::{}(n) => eval_{}(host, n, cx),",
                    type_name(label),
                    ident(pest_rule)
                );
            }
            LoweredVariant::Transparent { child: Some(c) } => {
                let _ = writeln!(
                    out,
                    "        {ty}::{}(n) => eval_{}(host, n, cx),",
                    type_name(c),
                    ident(c)
                );
            }
            LoweredVariant::Transparent { child: None } => {}
        }
    }

    if rule.recovers {
        let _ = writeln!(
            out,
            "        // A region the parse recovered from: report once, then\n\
            \x20       // poison, so one bad statement is one message rather than\n\
            \x20       // that plus every consequence of it (DESIGN.md §5.5).\n\
            \x20       {ty}::Error(span) => {{\n\
            \x20           cx.report(Diagnostic::error(\"could not parse this `{}`\").at(*span));\n\
            \x20           Err(Error::AlreadyReported)\n\
            \x20       }}",
            rule.name
        );
    }

    let _ = writeln!(out, "    }}\n}}\n");
    emit_eval_impl(out, &ty, &f);
}

fn emit_eval_struct(out: &mut String, ty: &str, f: &str, alt: &LoweredAlternative) {
    let ps = params(alt);

    let _ = writeln!(
        out,
        "/// Evaluates `{}`, from `{}`.\n\
         pub fn eval_{f}<H: Handlers>(host: &mut H, node: &{ty}, cx: &mut Ctx) -> Result<H::Out> {{\n\
        \x20   // Entering the node's span is what makes `cx.err(..)` inside the\n\
        \x20   // handler locate itself with no span bookkeeping (DESIGN.md §7).\n\
        \x20   cx.enter(node.span);\n\
        \x20   let result = eval_{f}_inner(host, node, cx);\n\
        \x20   cx.leave();\n\
        \x20   result\n\
         }}\n\n\
         fn eval_{f}_inner<H: Handlers>(\n\
        \x20   host: &mut H,\n\
        \x20   node: &{ty},\n\
        \x20   cx: &mut Ctx,\n\
         ) -> Result<H::Out> {{",
        alt.rule, alt.source
    );

    for p in &ps {
        let _ = writeln!(out, "    let {} = {};", ident(&p.name), p.extract);
    }

    let args: Vec<String> = ps.iter().map(|p| ident(&p.name)).collect();
    let call = if args.is_empty() {
        "cx".to_string()
    } else {
        format!("{}, cx", args.join(", "))
    };

    let _ = writeln!(out, "    host.{f}({call})\n}}\n");
    emit_eval_impl(out, ty, f);
}

fn emit_eval_impl(out: &mut String, ty: &str, f: &str) {
    let _ = writeln!(
        out,
        "impl Eval for {ty} {{\n\
        \x20   fn eval<H: Handlers>(&self, host: &mut H, cx: &mut Ctx) -> Result<H::Out> {{\n\
        \x20       eval_{f}(host, self, cx)\n\
        \x20   }}\n\
         }}\n"
    );
}

// ---------------------------------------------------------------------------
// The delegating macro
// ---------------------------------------------------------------------------

fn macro_ty(ty: &str) -> String {
    let ty = ty
        .replace(
            "Self::Out",
            "<Self as $crate::generated::dispatch::Semantics>::Out",
        )
        .replace("&Name", "&::nh_runtime::Name")
        .replace("[Name]", "[::nh_runtime::Name]");

    // A macro body is expanded in the *user's* crate, so an AST type has to be
    // named through `$crate` rather than left bare.
    qualify_rc(&ty)
}

/// Rewrites `Rc<Foo>` to its fully-qualified form.
///
/// Written by hand rather than with a regex because this crate has no regex
/// dependency and the shape is fixed: `Rc<` then one identifier then `>`.
fn qualify_rc(ty: &str) -> String {
    let mut out = String::new();
    let mut rest = ty;

    while let Some(at) = rest.find("Rc<") {
        out.push_str(&rest[..at]);
        let after = &rest[at + 3..];
        match after.find('>') {
            Some(close) => {
                let name = &after[..close];
                out.push_str("::std::rc::Rc<$crate::generated::ast::");
                out.push_str(name);
                out.push('>');
                rest = &after[close + 1..];
            }
            None => {
                out.push_str(&rest[at..]);
                return out;
            }
        }
    }

    out.push_str(rest);
    out
}

/// Rewrites a `crate::`-rooted path for use inside `macro_rules!`, where the
/// defining crate is spelled `$crate`.
fn macro_path(path: &str) -> String {
    match path.strip_prefix("crate::") {
        Some(rest) => format!("$crate::{rest}"),
        None => path.to_string(),
    }
}

/// The delegating impl, as a macro so the user names their type once.
fn emit_macro(out: &mut String, lowered: &Lowered, opts: &Options) {
    let _ = writeln!(
        out,
        "/// Writes the delegating `Handlers` impl for your type.\n\
         ///\n\
         /// ```ignore\n\
         /// nh_handlers!(Interp);\n\
         /// ```\n\
         ///\n\
         /// Each method body does nothing but call into `{}::<alternative>::run`,\n\
         /// so every handler is its own small file and the trait's exhaustiveness\n\
         /// still guarantees none is missing.\n\
         #[macro_export]\n\
         macro_rules! nh_handlers {{\n\
        \x20   ($host:ty) => {{\n\
        \x20       impl $crate::generated::dispatch::Handlers for $host {{",
        macro_path(&opts.handlers_path)
    );

    for alt in &lowered.alternatives {
        let m = ident(&alt.pest_rule);
        let module = &alt.pest_rule;
        let ps = params(alt);

        // Inside a macro the parameter types must be spelled with `$crate`, and
        // `Self::Out` becomes an explicit qualified path.
        let sig: String = ps
            .iter()
            .map(|p| {
                format!(
                    ",\n\x20               {}: {}",
                    ident(&p.name),
                    macro_ty(&p.ty)
                )
            })
            .collect();
        let args: Vec<String> = ps.iter().map(|p| ident(&p.name)).collect();
        let forward = if args.is_empty() {
            String::new()
        } else {
            format!(", {}", args.join(", "))
        };

        let _ = writeln!(
            out,
            "            fn {m}(\n\
            \x20               &mut self{sig},\n\
            \x20               cx: &mut ::nh_runtime::Ctx,\n\
            \x20           ) -> ::nh_runtime::Result<<Self as $crate::generated::dispatch::Semantics>::Out> {{\n\
            \x20               $crate::handlers::{module}::run(self{forward}, cx)\n\
            \x20           }}"
        );
    }

    let _ = writeln!(out, "        }}\n    }};\n}}\n");
}
