//! View generation.
//!
//! One struct per labelled alternative, with an accessor per binding. This is
//! where DESIGN.md §2 pays off: because bindings became node tags in the
//! `.pest`, accessors look children up **by name**, never `into_inner()`
//! positionally.
//!
//! **Views are mechanism, not interface.** Generated dispatch uses them to
//! build the parameters a handler receives; handlers themselves never see one
//! (DESIGN.md §5.4). The exception is `expr`, whose contents are a flat
//! operand/operator stream rather than a set of bindings — the operator driver
//! takes that view directly.
//!
//! Every lookup goes through `nh_runtime::node`, which scans direct children —
//! never `find_first_tagged`, which flattens the subtree and would let an outer
//! node pick up an inner one's identically-named tag.

use nh_lower::{Binding, Cardinality, Lowered, LoweredAlternative};
use std::fmt::Write as _;

use crate::{ident, type_name, Options, HEADER};

pub fn generate(lowered: &Lowered, opts: &Options) -> String {
    let mut out = String::new();
    out.push_str(HEADER);
    let _ = writeln!(
        out,
        "\n#![allow(dead_code, unused_imports)]\n\n\
         use nh_runtime::{{FileId, Ident, Node, View}};\n\
         use nh_runtime::pest::Pair;\n\n\
         use {}::Rule;\n",
        opts.parser_type.rsplit_once("::").map(|(m, _)| m).unwrap_or("crate")
    );

    for alt in &lowered.alternatives {
        emit_view(&mut out, alt);
    }

    // `expr` is not a labelled alternative — it comes from the operator table —
    // so it needs a view of its own, and only when the table produced one.
    if lowered.has_expr {
        emit_expr_view(&mut out);
    }

    out
}

/// A view over the flat `expr` rule.
///
/// It exposes the operand/operator stream rather than a folded tree, because
/// the driver that folds by precedence is M3. Once that lands this view becomes
/// an implementation detail of the driver.
fn emit_expr_view(out: &mut String) {
    let _ = writeln!(
        out,
        "/// View over `expr`, the rule supplied by the operator table.\n\
         ///\n\
         /// Exposes the flat operand/operator stream. Folding it by precedence is\n\
         /// the M3 driver's job; until then a language that needs expressions can\n\
         /// override `Handlers::expr` and walk this itself.\n\
         #[derive(Clone, Debug)]\n\
         pub struct ExprView<'i> {{\n\
        \x20   node: Node<'i, Rule>,\n\
         }}\n\n\
         impl<'i> View<'i, Rule> for ExprView<'i> {{\n\
        \x20   fn from_pair(pair: Pair<'i, Rule>, file: FileId) -> Self {{\n\
        \x20       ExprView {{ node: Node::new(pair, file) }}\n\
        \x20   }}\n\n\
        \x20   fn node(&self) -> &Node<'i, Rule> {{\n\
        \x20       &self.node\n\
        \x20   }}\n\
         }}\n\n\
         impl<'i> ExprView<'i> {{\n\
        \x20   /// Every direct child, operands and operators interleaved.\n\
        \x20   pub fn parts(&self) -> Vec<Node<'i, Rule>> {{\n\
        \x20       self.node.children().collect()\n\
        \x20   }}\n\n\
        \x20   /// The raw pairs the operator driver folds.\n\
        \x20   pub fn pairs(&self) -> Vec<Pair<'i, Rule>> {{\n\
        \x20       self.node.pair().clone().into_inner().collect()\n\
        \x20   }}\n\
         }}\n"
    );
}

fn emit_view(out: &mut String, alt: &LoweredAlternative) {
    let name = format!("{}View", type_name(&alt.pest_rule));

    let _ = writeln!(
        out,
        "/// One matched `{}`.\n\
         ///\n\
         /// From this alternative of `rule {}`:\n\
         ///\n\
         /// ```text\n\
         /// {}\n\
         /// ```",
        alt.pest_rule, alt.rule, alt.source
    );
    let _ = writeln!(
        out,
        "#[derive(Clone, Debug)]\n\
         pub struct {name}<'i> {{\n\
        \x20   node: Node<'i, Rule>,\n\
         }}\n\n\
         impl<'i> View<'i, Rule> for {name}<'i> {{\n\
        \x20   fn from_pair(pair: Pair<'i, Rule>, file: FileId) -> Self {{\n\
        \x20       {name} {{ node: Node::new(pair, file) }}\n\
        \x20   }}\n\n\
        \x20   fn node(&self) -> &Node<'i, Rule> {{\n\
        \x20       &self.node\n\
        \x20   }}\n\
         }}\n\n\
         // Accessors are inherent, so a binding named `text` or `span` shadows\n\
         // the `View` method of that name rather than colliding with it.\n\
         impl<'i> {name}<'i> {{"
    );

    let mut seen = Vec::new();
    for b in &alt.bindings {
        // A binding name can appear twice in one alternative (two branches of a
        // choice binding the same field). One accessor covers both.
        if seen.contains(&b.name) {
            continue;
        }
        seen.push(b.name.clone());
        emit_accessor(out, b);
    }

    let _ = writeln!(out, "}}\n");
}

/// Describes what a binding points at, and which handler parameter it becomes.
///
/// A view is *mechanism*: dispatch uses it to build the parameters a handler
/// actually receives (DESIGN.md §5.4). So the doc on an accessor says which
/// parameter it feeds rather than showing handler code — the handler has no
/// view to call this on.
fn usage(b: &Binding) -> (String, String) {
    let what = match (&b.token, &b.rule_ref) {
        (Some(t), _) if t.case_insensitive => {
            format!("the `{}` token, which folds case", t.name)
        }
        (Some(t), _) => format!("the `{}` token", t.name),
        (None, Some(rule)) => format!("the `{rule}` rule"),
        (None, None) => "a matched fragment".to_string(),
    };

    // The signature fragment goes on its own line so wrapping can never split
    // a type in half, which is unreadable and unsearchable.
    let p = crate::params::param(b);
    let how = format!(
        "/// Dispatch turns this into the handler parameter\n\
        \x20   /// `{}: {}`:\n\
        \x20   {}",
        p.name,
        p.ty,
        wrap(&p.doc, "    /// ")
    );

    (what, how)
}

/// Wraps a doc line, because a generated file is read far more often than the
/// grammar that produced it and rustfmt will not reflow a comment.
fn wrap(text: &str, prefix: &str) -> String {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.len() + 1 + word.len() > 74 - prefix.len() {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    lines.push(line);
    lines
        .iter()
        .map(|l| format!("{prefix}{l}"))
        .collect::<Vec<_>>()
        .join("\n")
        .trim_start()
        .to_string()
}

fn emit_accessor(out: &mut String, b: &Binding) {
    let method = ident(&b.name);
    let tag = &b.name;
    let folds = b.token.as_ref().is_some_and(|t| t.case_insensitive);
    let (what, how) = usage(b);

    // The doc leads with the grammar fragment, then what it is, then how to use
    // it — the three questions someone reading a handler actually has.
    let (ty, body, shape) = match (b.cardinality, folds) {
        (Cardinality::One, true) => (
            "Ident<'i, Rule>".to_string(),
            format!(
                "Ident::new(self.node.tagged(\"{tag}\").expect(\n\
                \x20           \"the grammar guarantees `{tag}` is present; regenerate if it changed\",\n\
                \x20       ))"
            ),
            String::new(),
        ),
        (Cardinality::One, false) => (
            "Node<'i, Rule>".to_string(),
            format!(
                "self.node.tagged(\"{tag}\").expect(\n\
                \x20           \"the grammar guarantees `{tag}` is present; regenerate if it changed\",\n\
                \x20       )"
            ),
            String::new(),
        ),
        (Cardinality::Optional, true) => (
            "Option<Ident<'i, Rule>>".to_string(),
            format!("self.node.tagged(\"{tag}\").map(Ident::new)"),
            "\n    /// Optional in the grammar (`?`), so this may be `None`.".to_string(),
        ),
        (Cardinality::Optional, false) => (
            "Option<Node<'i, Rule>>".to_string(),
            format!("self.node.tagged(\"{tag}\")"),
            "\n    /// Optional in the grammar (`?`), so this may be `None`.".to_string(),
        ),
        (Cardinality::Many, true) => (
            "Vec<Ident<'i, Rule>>".to_string(),
            format!(
                "self.node.tagged_all(\"{tag}\").into_iter().map(Ident::new).collect()"
            ),
            "\n    /// Repeated in the grammar (`*` or `+`), so this may be empty."
                .to_string(),
        ),
        (Cardinality::Many, false) => (
            "Vec<Node<'i, Rule>>".to_string(),
            format!("self.node.tagged_all(\"{tag}\")"),
            "\n    /// Repeated in the grammar (`*` or `+`), so this may be empty."
                .to_string(),
        ),
    };

    let _ = writeln!(
        out,
        "\n    /// `{tag}` — {what}.{shape}\n\
        \x20   ///\n\
        \x20   {how}\n\
        \x20   pub fn {method}(&self) -> {ty} {{\n\
        \x20       {body}\n\
        \x20   }}"
    );
}
