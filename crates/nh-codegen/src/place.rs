//! `Place` generation — assignable locations.
//!
//! DESIGN.md §6.8. The load-bearing property is that **payloads are
//! pre-evaluated**: a `Place` holds values, not thunks or unevaluated pairs.
//!
//! That is what makes `a[f()] += 1` call `f()` exactly once. If the place held
//! an unevaluated index, the compound-assignment default would force it twice —
//! once to read the current value and once to write the new one — and the bug
//! would stay invisible until someone put a side effect in a subscript.
//! Resolving the place up front makes double evaluation *unrepresentable*
//! rather than merely discouraged.

use nh_lower::{Binding, Cardinality, Lowered, LoweredAlternative};
use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::{ident, type_name, Options, HEADER};

/// A binding as it appears in a `Place` variant.
struct Field {
    name: String,
    /// Rust type of the field.
    ty: String,
    /// How the resolver produces it.
    source: Source,
}

enum Source {
    /// Read straight off the view — a name, not a value.
    Token { accessor: String },
    /// Evaluated once, here, before the place is handed over.
    Evaluated { accessor: String },
}

fn fields(alt: &LoweredAlternative) -> Vec<Field> {
    let mut out = Vec::new();
    let mut seen = Vec::new();

    for b in &alt.bindings {
        if seen.contains(&b.name) {
            continue;
        }
        seen.push(b.name.clone());
        out.push(field(b));
    }
    out
}

fn field(b: &Binding) -> Field {
    let accessor = ident(&b.name);
    match &b.token {
        Some(t) => {
            let base = if t.case_insensitive {
                "&'a Name"
            } else {
                "&'a str"
            };
            let ty = match b.cardinality {
                Cardinality::One => base.to_string(),
                Cardinality::Optional => format!("Option<{base}>"),
                Cardinality::Many => format!("Vec<{base}>"),
            };
            Field {
                name: b.name.clone(),
                ty,
                source: Source::Token { accessor },
            }
        }
        None => Field {
            name: b.name.clone(),
            ty: "Out".to_string(),
            source: Source::Evaluated { accessor },
        },
    }
}

pub fn generate(lowered: &Lowered, opts: &Options) -> String {
    let mut out = String::new();
    out.push_str(HEADER);

    let places: Vec<&LoweredAlternative> =
        lowered.alternatives.iter().filter(|a| a.place).collect();

    let _ = writeln!(out, "\n#![allow(dead_code, unused_imports)]\n");
    let _ = opts;

    // Without an operator table there is no `Expr`, so there is nothing an
    // assignment could target and no resolver to write. Emitting one anyway
    // would name types that do not exist.
    let resolvable = lowered.has_expr;

    let _ = writeln!(out, "use nh_runtime::{{Ctx, Error, Name, Result, Span}};\n");
    if resolvable {
        let _ = writeln!(
            out,
            "use super::ast::*;\n\
             use super::dispatch::{{eval_expr, Handlers}};\n"
        );
    }

    // An expression binding in a place must resolve to exactly one value, or
    // "pre-evaluated" has no meaning. Say so at build time rather than
    // generating something that compiles and misbehaves.
    for alt in &places {
        for b in &alt.bindings {
            if b.token.is_none() && b.cardinality != Cardinality::One {
                let _ = writeln!(
                    out,
                    "compile_error!(\n\
                    \x20   \"`{}` is marked `place`, but its binding `{}` is optional or \\\n\
                    \x20    repeated. An assignable location must resolve to exactly one \\\n\
                    \x20    value per field, because the place is evaluated once up front.\"\n\
                     );",
                    alt.pest_rule, b.name
                );
            }
        }
    }

    emit_enum(&mut out, &places);
    if resolvable {
        emit_resolver(&mut out, &places);
    }

    out
}

fn emit_enum(out: &mut String, places: &[&LoweredAlternative]) {
    let _ = writeln!(
        out,
        "/// An assignable location.\n\
         ///\n\
         /// One variant per `place`-marked alternative in the grammar. Fields that\n\
         /// name something (an identifier) arrive as nodes; fields that are\n\
         /// expressions arrive **already evaluated**, exactly once.\n\
         #[derive(Debug)]\n\
         pub enum Place<'a, Out> {{"
    );

    if places.is_empty() {
        let _ = writeln!(
            out,
            "    /// This grammar marks no alternative `place`, so nothing is\n\
            \x20   /// assignable and this enum has no inhabitants.\n\
            \x20   Never(::core::marker::PhantomData<(&'a (), Out)>),"
        );
    }

    for alt in places {
        let variant = type_name(&alt.pest_rule);
        let fs = fields(alt);

        let _ = writeln!(
            out,
            "    /// From `{}` (`-> {} place`).\n\
            \x20   {variant} {{\n\
            \x20       /// The whole target, for diagnostics.\n\
            \x20       span: Span,",
            alt.pest_rule, alt.label
        );
        for f in &fs {
            let doc = match f.source {
                Source::Token { .. } => "a name, not a value",
                Source::Evaluated { .. } => "evaluated once, when the place was resolved",
            };
            let _ = writeln!(out, "        /// `{}` — {doc}.\n        {}: {},", f.name, ident(&f.name), f.ty);
        }
        // Keep `'i` and `Out` used even when a variant happens not to need one.
        if !fs.iter().any(|f| matches!(f.source, Source::Token { .. })) {
            let _ = writeln!(
                out,
                "        #[doc(hidden)]\n        _marker: ::core::marker::PhantomData<&'a ()>,"
            );
        }
        if !fs.iter().any(|f| matches!(f.source, Source::Evaluated { .. })) {
            let _ = writeln!(
                out,
                "        #[doc(hidden)]\n        _out: ::core::marker::PhantomData<Out>,"
            );
        }
        let _ = writeln!(out, "    }},");
    }

    let _ = writeln!(out, "}}\n");

    // span()
    let _ = writeln!(
        out,
        "impl<'a, Out> Place<'a, Out> {{\n\
        \x20   /// Where the target appears in the source.\n\
        \x20   pub fn span(&self) -> Span {{\n\
        \x20       match self {{"
    );
    if places.is_empty() {
        let _ = writeln!(
            out,
            "            Place::Never(_) => unreachable!(\"no place variants exist\"),"
        );
    }
    for alt in places {
        let variant = type_name(&alt.pest_rule);
        let _ = writeln!(out, "            Place::{variant} {{ span, .. }} => *span,");
    }
    let _ = writeln!(out, "        }}\n    }}\n}}\n");
}

fn emit_resolver(out: &mut String, places: &[&LoweredAlternative]) {
    if places.is_empty() {
        // A degenerate resolver would carry an unused `host` and a single-arm
        // match — clippy warnings in a file the user does not own and cannot
        // fix (DESIGN.md §11, standing constraint 6).
        let _ = writeln!(
            out,
            "/// This grammar marks nothing `place`, so nothing is assignable.\n\
             pub fn resolve_place<'a, H: Handlers>(\n\
            \x20   _host: &mut H,\n\
            \x20   _node: &'a Expr,\n\
            \x20   _cx: &mut Ctx,\n\
             ) -> Result<Place<'a, H::Out>> {{\n\
            \x20   Err(Error::runtime(\n\
            \x20       \"nothing is assignable; this grammar marks no alternative `place`\",\n\
            \x20   ))\n\
             }}\n"
        );
        return;
    }

    // Places live under `atom`, so the entry point unwraps an expression and
    // hands the atom to the resolver for whichever rule owns the alternatives.
    let mut by_rule: BTreeMap<&str, Vec<&LoweredAlternative>> = BTreeMap::new();
    for alt in places {
        by_rule.entry(alt.rule.as_str()).or_default().push(alt);
    }

    let first = by_rule.keys().next().copied().unwrap_or("atom");

    let _ = writeln!(
        out,
        "/// Resolves an assignment target into a [`Place`].\n\
         ///\n\
         /// The target is **not** evaluated as a value — that is the whole reason\n\
         /// assignment is lazy in its left operand. Expression fields inside the\n\
         /// place *are* evaluated, once, here.\n\
         pub fn resolve_place<'a, H: Handlers>(\n\
        \x20   host: &mut H,\n\
        \x20   node: &'a Expr,\n\
        \x20   cx: &mut Ctx,\n\
         ) -> Result<Place<'a, H::Out>> {{\n\
        \x20   match node {{\n\
        \x20       Expr::Atom(a) => resolve_place_{}(host, a, cx),\n\
        \x20       Expr::Prefix {{ span, .. }}\n\
        \x20       | Expr::Postfix {{ span, .. }}\n\
        \x20       | Expr::Infix {{ span, .. }} => Err(Error::runtime(\n\
        \x20           \"this is not an assignable target\",\n\
        \x20       )\n\
        \x20       .at(*span)),\n\
        \x20   }}\n\
         }}\n",
        ident(first)
    );

    for (rule, alts) in &by_rule {
        let ty = type_name(rule);
        let _ = writeln!(
            out,
            "fn resolve_place_{}<'a, H: Handlers>(\n\
            \x20   host: &mut H,\n\
            \x20   node: &'a {ty},\n\
            \x20   cx: &mut Ctx,\n\
             ) -> Result<Place<'a, H::Out>> {{\n\
            \x20   match node {{",
            ident(rule)
        );

        for alt in alts {
            let variant = type_name(&alt.pest_rule);
            let fs = fields(alt);
            let _ = writeln!(
                out,
                "        {ty}::{}(n) => {{\n\
                \x20           let span = n.span;",
                type_name(&alt.label)
            );

            for f in &fs {
                match &f.source {
                    Source::Token { accessor } => {
                        let _ = writeln!(out, "            let {} = &n.{accessor};", ident(&f.name));
                    }
                    Source::Evaluated { accessor } => {
                        let _ = writeln!(
                            out,
                            "            // Evaluated exactly once: this is what keeps `a[f()] += 1`\n\
                            \x20           // from calling `f()` twice.\n\
                            \x20           let {} = eval_expr(host, &n.{accessor}, cx)?;",
                            ident(&f.name)
                        );
                    }
                }
            }

            let mut inits: Vec<String> = vec!["span".to_string()];
            inits.extend(fs.iter().map(|f| ident(&f.name)));
            if !fs.iter().any(|f| matches!(f.source, Source::Token { .. })) {
                inits.push("_marker: ::core::marker::PhantomData".to_string());
            }
            if !fs.iter().any(|f| matches!(f.source, Source::Evaluated { .. })) {
                inits.push("_out: ::core::marker::PhantomData".to_string());
            }

            let _ = writeln!(
                out,
                "            Ok(Place::{variant} {{ {} }})\n        }}",
                inits.join(", ")
            );
        }

        // A parenthesised target: `(a) = 1` still names `a`.
        let _ = writeln!(
            out,
            "        {ty}::Expr(e) => resolve_place(host, e, cx),\n\
            \x20       _ => Err(Error::runtime(\n\
            \x20           \"this is not an assignable target; mark the alternative `place` \\\n\
            \x20            in the grammar\",\n\
            \x20       )),\n\
            \x20   }}\n\
             }}\n"
        );
    }
}
