//! Owned AST generation (M7).
//!
//! Handlers receive *values*, not nodes — that is the reducer model and it does
//! not change. What changes is what the evaluator walks. A `pest::Pair` borrows
//! the input, so a `lazy` binding could only be forced during the handler call
//! that received it: it could not be stored, and it could not be inspected.
//! That made `GOTO`, subroutines, and closures inexpressible — a language
//! cannot keep a piece of its own program for later (DESIGN.md §9).
//!
//! So the parse tree is converted once, up front, into owned types generated
//! from the grammar. Every rule-typed field is an `Shared`, which makes recursive
//! types finite, node sharing free, and a `lazy` binding storable anywhere.
//!
//! ```text
//! rule stmt = "let" name:IDENT "=" value:expr ";" -> bind
//!           | value:expr ";"                      -> eval
//!           ;
//! ```
//!
//! ```ignore
//! pub enum Stmt { Bind(Shared<StmtBind>), Eval(Shared<StmtEval>) }
//! pub struct StmtBind { pub name: String, pub value: Shared<Expr>, pub span: Span }
//! ```

use nh_lower::{Binding, Cardinality, Lowered, LoweredAlternative, LoweredRule, LoweredVariant, RuleShape};
use std::collections::HashMap;
use std::fmt::Write as _;

use crate::{ident, type_name, Options, HEADER};

pub fn generate(lowered: &Lowered, _opts: &Options) -> String {
    let mut out = String::new();
    out.push_str(HEADER);

    // Only a case-folding token binding produces a `Name` field, so a grammar
    // without one must not import it (DESIGN.md §11, standing constraint 6).
    let folds = lowered
        .alternatives
        .iter()
        .flat_map(|a| a.bindings.iter())
        .any(|b| b.token.as_ref().is_some_and(|t| t.case_insensitive));
    let imports = if folds { "{Name, Span}" } else { "Span" };

    let _ = writeln!(
        out,
        "\n#![allow(dead_code)]\n\n\
         use nh_runtime::Shared;\n\n\
         use nh_runtime::{imports};\n"
    );

    let by_name: HashMap<&str, &LoweredRule> =
        lowered.rules.iter().map(|r| (r.name.as_str(), r)).collect();
    let alts: HashMap<&str, &LoweredAlternative> = lowered
        .alternatives
        .iter()
        .map(|a| (a.pest_rule.as_str(), a))
        .collect();

    if lowered.has_expr {
        emit_expr(&mut out, &by_name, lowered);
    }

    for rule in &lowered.rules {
        emit_rule(&mut out, rule, &by_name, &alts, lowered);
    }

    emit_builders(&mut out, lowered, &by_name, &alts);

    out
}

/// The folded expression tree.
///
/// `expr` parses as a *flat* operand/operator stream (DESIGN.md §5.2) and the
/// driver folds it by precedence. Folding happens once, here, while the AST is
/// built — not on every evaluation. That is what makes a `WHILE` condition
/// cheap to re-test: the tree is already shaped.
fn emit_expr(out: &mut String, by_name: &HashMap<&str, &LoweredRule>, lowered: &Lowered) {
    let atom = lowered
        .rules
        .iter()
        .find(|r| r.name == "atom")
        .map(|r| resolved_type(&r.name, by_name))
        .unwrap_or_else(|| "Atom".to_string());

    let _ = writeln!(
        out,
        "/// An expression, already folded by precedence.\n\
         ///\n\
         /// The grammar parses operators as a flat stream; the shape below is\n\
         /// what the driver folded it into, once, when the AST was built.\n\
         #[derive(Clone, Debug)]\n\
         pub enum Expr {{\n\
        \x20   Atom(Shared<{atom}>),\n\
        \x20   Prefix {{ op: OpKind, operand: Shared<Expr>, span: Span }},\n\
        \x20   Postfix {{ operand: Shared<Expr>, op: OpKind, span: Span }},\n\
        \x20   Infix {{ lhs: Shared<Expr>, op: OpKind, rhs: Shared<Expr>, span: Span }},\n\
         }}\n\n\
         /// Which operator an [`Expr`] node applies, as a generated rule id.\n\
         pub type OpKind = crate::Rule;\n"
    );
}

fn emit_rule(
    out: &mut String,
    rule: &LoweredRule,
    by_name: &HashMap<&str, &LoweredRule>,
    alts: &HashMap<&str, &LoweredAlternative>,
    lowered: &Lowered,
) {
    let name = type_name(&rule.name);

    match &rule.shape {
        // An alias carries no node of its own. Emitting a wrapper would put it
        // in every signature that mentions the rule, for nothing.
        RuleShape::Alias { child } => {
            // No child means nothing to delegate to. Lowering rejects almost
            // every way of getting here, so this is a backstop rather than the
            // diagnostic — but it has to say something, because what it used to
            // emit was `pub type X = Unresolved;` and a call to a `build_x` it
            // never defined. That surfaced as two "cannot find" errors in
            // generated code, naming neither the rule nor the fix.
            let Some(child) = child.as_deref() else {
                let _ = writeln!(
                    out,
                    "compile_error!(\n\
                    \x20   \"`rule {}` has no `-> label` and no single child to \\\n\
                    \x20    stand in for, so there is no type it could be. Give it a \\\n\
                    \x20    `-> label` so it gets a node of its own.\"\n\
                     );\n",
                    rule.name
                );
                return;
            };
            let _ = writeln!(
                out,
                "/// `rule {}` delegates to `{child}`, so it is that type.\n\
                 pub type {name} = {};\n",
                rule.name,
                resolved_type(child, by_name)
            );
        }

        RuleShape::Single { pest_rule } => {
            let Some(alt) = alts.get(pest_rule.as_str()) else {
                return;
            };
            emit_struct(out, &name, alt, by_name, lowered);
        }

        RuleShape::Choice(variants) => {
            let _ = writeln!(out, "/// `rule {}`.", rule.name);
            let _ = writeln!(out, "#[derive(Clone, Debug)]");
            let _ = writeln!(out, "pub enum {name} {{");

            for v in variants {
                match v {
                    LoweredVariant::Labelled { label, pest_rule } => {
                        let vname = type_name(label);
                        let tname = type_name(pest_rule);
                        let _ = writeln!(out, "    {vname}(Shared<{tname}>),");
                    }
                    LoweredVariant::Transparent { child: Some(c) } => {
                        let t = resolved_type(c, by_name);
                        let _ = writeln!(
                            out,
                            "    /// A transparent alternative yielding `{c}`.\n\
                            \x20   {}(Shared<{t}>),",
                            type_name(c)
                        );
                    }
                    // Already a runtime error today: a transparent alternative
                    // with no single child has nothing to delegate to.
                    LoweredVariant::Transparent { child: None } => {}
                }
            }

            if rule.recovers {
                let _ = writeln!(
                    out,
                    "    /// A span the parser recovered from. Reported once, then poisoned.\n\
                    \x20   Error(Span),"
                );
            }
            let _ = writeln!(out, "}}\n");

            // The sub-structs the variants point at.
            for v in variants {
                if let LoweredVariant::Labelled { pest_rule, .. } = v {
                    if let Some(alt) = alts.get(pest_rule.as_str()) {
                        emit_struct(out, &type_name(pest_rule), alt, by_name, lowered);
                    }
                }
            }
        }
    }
}

fn emit_struct(
    out: &mut String,
    name: &str,
    alt: &LoweredAlternative,
    by_name: &HashMap<&str, &LoweredRule>,
    lowered: &Lowered,
) {
    let _ = writeln!(
        out,
        "/// From `rule {} = {}`.\n\
         #[derive(Clone, Debug)]\n\
         pub struct {name} {{",
        alt.rule, alt.source
    );

    let mut seen = Vec::new();
    for b in &alt.bindings {
        if seen.contains(&b.name) {
            continue;
        }
        seen.push(b.name.clone());
        let _ = writeln!(
            out,
            "    pub {}: {},",
            ident(&b.name),
            field_type(b, by_name, lowered)
        );
    }

    let _ = writeln!(out, "    pub span: Span,\n}}\n");
}

/// The owned type a binding becomes.
///
/// Every rule-typed field is an `Shared`: that makes the recursion finite, sharing
/// free, and — the point of the whole exercise — a `lazy` binding storable on
/// the interpreter long after the handler that received it returned.
fn field_type(b: &Binding, by_name: &HashMap<&str, &LoweredRule>, lowered: &Lowered) -> String {
    let inner = match (&b.token, &b.rule_ref) {
        // A folding token keeps both spellings: `.key()` to look up, `.text()`
        // to report. Losing either is a bug the type can prevent.
        (Some(t), _) if t.case_insensitive => "Name".to_string(),
        (Some(_), _) => "String".to_string(),
        (None, Some(rule)) => format!("Shared<{}>", rule_type(rule, by_name, lowered)),
        // A group or a sequence: the text it matched is all there is to keep.
        (None, None) => "String".to_string(),
    };

    match b.cardinality {
        Cardinality::One => inner,
        Cardinality::Optional => format!("Option<{inner}>"),
        Cardinality::Many => format!("Vec<{inner}>"),
    }
}

/// `expr` is not a declared rule — it comes from the operator table — but it is
/// dispatched like one and needs a type like one.
fn rule_type(rule: &str, by_name: &HashMap<&str, &LoweredRule>, lowered: &Lowered) -> String {
    if rule == "expr" && lowered.has_expr {
        return "Expr".to_string();
    }
    resolved_type(rule, by_name)
}

/// Follows aliases to the type that actually carries the node.
///
/// `rule atom = primary;` means an `atom` field is really a `Primary`, and
/// saying so keeps a chain of empty wrappers out of the generated types.
fn resolved_type(rule: &str, by_name: &HashMap<&str, &LoweredRule>) -> String {
    let mut name = rule;
    let mut hops = 0;
    while let Some(r) = by_name.get(name) {
        match &r.shape {
            RuleShape::Alias { child: Some(c) } if hops < 16 => {
                name = c;
                hops += 1;
            }
            _ => break,
        }
    }
    type_name(name)
}

// ---------------------------------------------------------------------------
// The builder
// ---------------------------------------------------------------------------

/// Emits the parse-tree → AST conversion.
///
/// This runs **once**, before evaluation, which is what buys the whole design:
/// after it, nothing borrows the parse tree, so a piece of program can be kept,
/// shared, and re-run. It is also where operators are folded, so a loop that
/// re-tests its condition is not re-folding an expression every pass.
fn emit_builders(
    out: &mut String,
    lowered: &Lowered,
    by_name: &HashMap<&str, &LoweredRule>,
    alts: &HashMap<&str, &LoweredAlternative>,
) {
    let _ = writeln!(
        out,
        "// ---------------------------------------------------------------------------\n\
         // Building the tree\n\
         // ---------------------------------------------------------------------------\n\n\
         use nh_runtime::pest::Pair;\n\
         use nh_runtime::{{Error, FileId, Result, View}};\n\n\
         use super::views::*;\n\
         use crate::Rule;\n\n\
         fn span_of(pair: &Pair<'_, Rule>, file: FileId) -> Span {{\n\
        \x20   let s = pair.as_span();\n\
        \x20   Span::new(file, s.start() as u32, s.end() as u32)\n\
         }}\n\n\
         /// Descends through a rule that carries no node of its own.\n\
         ///\n\
         /// A wrapper with more than one child means the author meant to handle\n\
         /// it, so this says that rather than silently picking the first.\n\
         fn only_child(pair: Pair<'_, Rule>) -> Result<Pair<'_, Rule>> {{\n\
        \x20   let rule = pair.as_rule();\n\
        \x20   let mut inner = pair.into_inner();\n\
        \x20   match (inner.next(), inner.next()) {{\n\
        \x20       (Some(only), None) => Ok(only),\n\
        \x20       (Some(_), Some(_)) => Err(Error::runtime(format!(\n\
        \x20           \"`{{rule:?}}` produced several children and has no handler; \\\n\
        \x20            add a `-> label` to the alternative you meant\"\n\
        \x20       ))),\n\
        \x20       (None, _) => Err(Error::runtime(format!(\n\
        \x20           \"`{{rule:?}}` produced no children\"\n\
        \x20       ))),\n\
        \x20   }}\n\
         }}\n"
    );

    if lowered.has_expr {
        emit_expr_builder(out, lowered);
    }

    for rule in &lowered.rules {
        match &rule.shape {
            // An alias has no type and no builder: callers resolve through it
            // and call the builder of whatever it delegates to.
            RuleShape::Alias { .. } => {}
            RuleShape::Single { pest_rule } => {
                if let Some(alt) = alts.get(pest_rule.as_str()) {
                    emit_struct_builder(out, &type_name(&rule.name), alt, by_name, lowered);
                }
            }
            RuleShape::Choice(variants) => {
                emit_choice_builder(out, rule, variants, by_name);
                for v in variants {
                    if let LoweredVariant::Labelled { pest_rule, .. } = v {
                        if let Some(alt) = alts.get(pest_rule.as_str()) {
                            emit_struct_builder(
                                out,
                                &type_name(pest_rule),
                                alt,
                                by_name,
                                lowered,
                            );
                        }
                    }
                }
            }
        }
    }
}

fn emit_expr_builder(out: &mut String, lowered: &Lowered) {
    let atom = lowered
        .rules
        .iter()
        .find(|r| r.name == "atom")
        .and_then(|r| match &r.shape {
            RuleShape::Alias { child } => child.clone(),
            _ => Some(r.name.clone()),
        })
        .unwrap_or_else(|| "atom".to_string());

    let _ = writeln!(
        out,
        "/// Folds the flat operand/operator stream into a tree, once.\n\
         pub fn build_expr(pair: Pair<'_, Rule>, file: FileId) -> Result<Shared<Expr>> {{\n\
        \x20   let span = span_of(&pair, file);\n\
        \x20   let tree = nh_runtime::ops::build(\n\
        \x20       pair.into_inner().collect(),\n\
        \x20       super::dispatch::op_info,\n\
        \x20       super::dispatch::prefix_info,\n\
        \x20   )\n\
        \x20   .map_err(|e| Error::runtime(e.to_string()).at(span))?;\n\
        \x20   from_op_tree(&tree, file)\n\
         }}\n\n\
         fn from_op_tree(\n\
        \x20   tree: &nh_runtime::OpTree<'_, Rule>,\n\
        \x20   file: FileId,\n\
         ) -> Result<Shared<Expr>> {{\n\
        \x20   use nh_runtime::OpTree;\n\
        \x20   Ok(Shared::new(match tree {{\n\
        \x20       OpTree::Atom(p) => Expr::Atom(build_{atom}(p.clone(), file)?),\n\
        \x20       OpTree::Prefix {{ op, operand }} => Expr::Prefix {{\n\
        \x20           op: op.as_rule(),\n\
        \x20           operand: from_op_tree(operand, file)?,\n\
        \x20           span: span_of(op, file),\n\
        \x20       }},\n\
        \x20       OpTree::Postfix {{ operand, op }} => Expr::Postfix {{\n\
        \x20           operand: from_op_tree(operand, file)?,\n\
        \x20           op: op.as_rule(),\n\
        \x20           span: span_of(op, file),\n\
        \x20       }},\n\
        \x20       OpTree::Infix {{ lhs, op, rhs }} => Expr::Infix {{\n\
        \x20           lhs: from_op_tree(lhs, file)?,\n\
        \x20           op: op.as_rule(),\n\
        \x20           rhs: from_op_tree(rhs, file)?,\n\
        \x20           span: span_of(op, file),\n\
        \x20       }},\n\
        \x20   }}))\n\
         }}\n"
    );
}

/// A choice descends to the child that carries the node, then names it.
fn emit_choice_builder(
    out: &mut String,
    rule: &LoweredRule,
    variants: &[LoweredVariant],
    by_name: &HashMap<&str, &LoweredRule>,
) {
    let name = type_name(&rule.name);
    let fname = builder_name(&rule.name);

    let _ = writeln!(
        out,
        "/// Builds a `{}` node.\n\
         pub fn {fname}(pair: Pair<'_, Rule>, file: FileId) -> Result<Shared<{name}>> {{\n\
        \x20   let mut pair = pair;\n\
        \x20   loop {{\n\
        \x20       let span = span_of(&pair, file);\n\
        \x20       match pair.as_rule() {{",
        rule.name
    );

    for v in variants {
        match v {
            LoweredVariant::Labelled { label, pest_rule } => {
                let _ = writeln!(
                    out,
                    "            Rule::{pest_rule} => {{\n\
                    \x20               return Ok(Shared::new({name}::{}({}(pair, file)?)))\n\
                    \x20           }}",
                    type_name(label),
                    builder_name(pest_rule)
                );
            }
            LoweredVariant::Transparent { child: Some(c) } => {
                let target = resolved_rule(c, by_name);
                let _ = writeln!(
                    out,
                    "            Rule::{c} => {{\n\
                    \x20               return Ok(Shared::new({name}::{}({}(pair, file)?)))\n\
                    \x20           }}",
                    type_name(c),
                    builder_name(&target)
                );
            }
            LoweredVariant::Transparent { child: None } => {}
        }
    }

    if rule.recovers {
        let _ = writeln!(
            out,
            "            Rule::nh_error_{} => return Ok(Shared::new({name}::Error(span))),",
            rule.pest_rule
        );
    }

    let _ = writeln!(
        out,
        "            // A wrapper rule: the node is one level down.\n\
        \x20           _ => pair = only_child(pair).map_err(|e| e.at(span))?,\n\
        \x20       }}\n\
        \x20   }}\n\
         }}\n"
    );
}

fn emit_struct_builder(
    out: &mut String,
    name: &str,
    alt: &LoweredAlternative,
    by_name: &HashMap<&str, &LoweredRule>,
    lowered: &Lowered,
) {
    let fname = builder_name(&alt.pest_rule);
    let view = format!("{}View", type_name(&alt.pest_rule));

    let _ = writeln!(
        out,
        "/// Builds `{}` from `{}`.\n\
         pub fn {fname}(pair: Pair<'_, Rule>, file: FileId) -> Result<Shared<{name}>> {{\n\
        \x20   let view = {view}::from_pair(pair, file);\n\
        \x20   Ok(Shared::new({name} {{",
        name, alt.source
    );

    let mut seen = Vec::new();
    for b in &alt.bindings {
        if seen.contains(&b.name) {
            continue;
        }
        seen.push(b.name.clone());
        let _ = writeln!(
            out,
            "        {}: {},",
            ident(&b.name),
            field_build(b, by_name, lowered)
        );
    }

    let _ = writeln!(out, "        span: view.span(),\n    }}))\n}}\n");
}

/// How one binding's value is produced from its view accessor.
fn field_build(b: &Binding, by_name: &HashMap<&str, &LoweredRule>, lowered: &Lowered) -> String {
    let accessor = format!("view.{}()", ident(&b.name));

    // `{n}` stands for one node, so the same expression serves all three
    // cardinalities.
    let one = match (&b.token, &b.rule_ref) {
        (Some(t), _) if t.case_insensitive => "Name::new({n}.text(), {n}.span())".to_string(),
        (Some(_), _) | (None, None) => "{n}.text().to_string()".to_string(),
        (None, Some(rule)) => {
            let target = if rule == "expr" && lowered.has_expr {
                "expr".to_string()
            } else {
                resolved_rule(rule, by_name)
            };
            format!("{}({{n}}.into_pair(), file)?", builder_name(&target))
        }
    };

    match b.cardinality {
        Cardinality::One => one.replace("{n}", &accessor),
        // `.map`, not a `match`: the match form is a clippy warning in a file
        // the reader cannot edit (DESIGN.md §11, standing constraint 6).
        Cardinality::Optional if !one.contains('?') => format!(
            "{accessor}.map(|n| {})",
            one.replace("{n}", "n")
        ),
        // ...except when building the value can fail, since `?` cannot cross a
        // closure boundary.
        Cardinality::Optional => format!(
            "match {accessor} {{ Some(n) => Some({}), None => None }}",
            one.replace("{n}", "n")
        ),
        Cardinality::Many => format!(
            "{{ let mut v = Vec::new(); for n in {accessor} {{ v.push({}); }} v }}",
            one.replace("{n}", "n")
        ),
    }
}

fn builder_name(rule: &str) -> String {
    format!("build_{}", ident(rule))
}

/// Follows aliases to the rule that actually carries the node, so a caller
/// calls the builder that exists rather than one that was never emitted.
fn resolved_rule(rule: &str, by_name: &HashMap<&str, &LoweredRule>) -> String {
    let mut name = rule;
    let mut hops = 0;
    while let Some(r) = by_name.get(name) {
        match &r.shape {
            RuleShape::Alias { child: Some(c) } if hops < 16 => {
                name = c;
                hops += 1;
            }
            _ => break,
        }
    }
    name.to_string()
}
