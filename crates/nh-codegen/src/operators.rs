//! Generating the operator table, the evaluator, and role signatures.
//!
//! The table itself was resolved at build time by `nh-operators`; what is
//! emitted here is the runtime half:
//!
//! * `op_info` — `Rule` → precedence, fixity, associativity, feeding the
//!   [`nh_runtime::ops`] precedence-climbing builder.
//! * `eval_tree` — walks the folded tree, calling one `Operators` method per
//!   node. Strict operands are evaluated before the call; **lazy ones are
//!   handed over as a [`Deferred`] and evaluated only if the handler forces
//!   them**, which is what makes `&&` short-circuit (DESIGN.md §6.6).
//! * Grouped-role discriminants — when a whole tier binds one role, the method
//!   takes an enum instead of the tier producing six near-identical methods.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use nh_operators::{OperatorTable, Operator, Tier};
use nh_syntax::ast::Fixity;

use crate::{ident, type_name};

/// One operator, resolved for codegen.
pub struct Emitted<'a> {
    pub rule: String,
    pub op: &'a Operator,
    pub tier: &'a Tier,
    /// Precedence: higher binds tighter.
    pub precedence: u16,
    /// The role's method name.
    pub role: String,
    /// Discriminant variant, when the tier shares one role.
    pub variant: Option<String>,
}

/// Resolves every operator to the pest rule name the lowerer emitted for it.
///
/// The lowerer names operator rules from their spelling, so this must use the
/// same derivation or the table would reference rules that do not exist.
pub fn emitted<'a>(table: &'a OperatorTable) -> Vec<Emitted<'a>> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for (i, tier) in table.tiers.iter().enumerate() {
        for op in &tier.operators {
            // The lowerer emits one rule per distinct literal, shared across
            // fixities, so `-` has a single rule used by both `sub` and `neg`.
            let rule = format!("nh_op_{}", nh_lower::names::symbolic(&op.literal));
            if !seen.insert((rule.clone(), tier.fixity)) {
                continue;
            }
            out.push(Emitted {
                rule,
                op,
                tier,
                // Tier 0 is the loosest tier and the builder treats a HIGHER
                // number as binding tighter, so precedence is the tier index.
                // Getting this backwards makes `&&` bind tighter than `>`,
                // which parses `a > 10 && b > 100` as `(a > 10 && b) > 100`.
                precedence: i as u16 + 1,
                role: tier.grouped_role.clone().unwrap_or_else(|| op.role.clone()),
                variant: tier
                    .grouped_role
                    .as_ref()
                    .map(|_| type_name(&nh_lower::names::symbolic(&op.literal))),
            });
        }
    }
    out
}

/// Roles that take a discriminant, mapped to their variants.
pub fn grouped_roles(ops: &[Emitted<'_>]) -> BTreeMap<String, Vec<(String, String)>> {
    let mut out: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for e in ops {
        if let Some(variant) = &e.variant {
            let entry = out.entry(e.role.clone()).or_default();
            if !entry.iter().any(|(v, _)| v == variant) {
                entry.push((variant.clone(), e.op.literal.clone()));
            }
        }
    }
    // A tier that binds one role but holds a single operator gains nothing from
    // a discriminant — `-> pow` on `**` alone should be `pow(lhs, rhs)`, not
    // `pow(lhs, PowOp::StarStar, rhs)`.
    out.retain(|_, variants| variants.len() > 1);
    out
}

/// Whether a role defers its right operand.
fn lazy_rhs(op: &Operator) -> bool {
    op.lazy.iter().any(|l| l == "rhs")
}

/// Whether a role defers its *left* operand — assignment needs a place, not a
/// value, so the driver must not evaluate it.
fn lazy_lhs(op: &Operator) -> bool {
    op.lazy.iter().any(|l| l == "lhs")
}

/// The short-circuit condition for a lazy role, as a Rust expression over
/// `lhs`.
///
/// This is role-specific and getting it wrong is silent: `||` returns its left
/// operand when that operand is *truthy*, which is the exact opposite of `&&`.
fn short_circuit_when(role: &str) -> &'static str {
    match role {
        "or_else" => "self.truthy(&lhs)",
        "coalesce" => "!self.is_null(&lhs)",
        // `and_then` and anything else lazy in rhs: stop on a falsy left.
        _ => "!self.truthy(&lhs)",
    }
}

// ---------------------------------------------------------------------------
// Emission
// ---------------------------------------------------------------------------

pub fn emit_discriminants(out: &mut String, ops: &[Emitted<'_>]) {
    let grouped = grouped_roles(ops);
    if grouped.is_empty() {
        return;
    }

    let _ = writeln!(
        out,
        "// ---------------------------------------------------------------------------\n\
         // Grouped-role discriminants\n\
         //\n\
         // Several operators binding one role produce a single trait method taking\n\
         // this enum, instead of six near-identical methods (DESIGN.md §6.3).\n\
         // ---------------------------------------------------------------------------"
    );

    for (role, variants) in &grouped {
        let name = format!("{}Op", type_name(role));
        let _ = writeln!(out, "\n#[derive(Clone, Copy, PartialEq, Eq, Debug)]\npub enum {name} {{");
        for (variant, literal) in variants {
            let _ = writeln!(out, "    /// `{literal}`\n    {variant},");
        }
        let _ = writeln!(out, "}}\n");

        let _ = writeln!(
            out,
            "impl {name} {{\n\
            \x20   /// The operator as written in the source language.\n\
            \x20   pub fn spelling(self) -> &'static str {{\n\
            \x20       match self {{"
        );
        for (variant, literal) in variants {
            let _ = writeln!(out, "            {name}::{variant} => {literal:?},");
        }
        let _ = writeln!(out, "        }}\n    }}\n}}");
    }
    out.push('\n');
}

/// The `Operators` trait: one defaulted method per role.
pub fn emit_trait(out: &mut String, ops: &[Emitted<'_>]) {
    let grouped = grouped_roles(ops);

    let _ = writeln!(
        out,
        "/// Operator semantics.\n\
         ///\n\
         /// Every method is defaulted to an `unsupported` error, so a language\n\
         /// implements only the operators it actually has and gets the rest of the\n\
         /// table's parsing, precedence, and short-circuiting for free (§6.4).\n\
         pub trait Operators: Semantics {{"
    );

    if ops.is_empty() {
        let _ = writeln!(out, "    // This grammar declares no operators.");
    }

    // One method per role, keyed so a role shared across tiers emits once.
    let mut done = std::collections::BTreeSet::new();
    for e in ops {
        if !done.insert((e.role.clone(), e.tier.fixity)) {
            continue;
        }
        emit_method(out, e, &grouped);
    }

    emit_assignment(out, ops, &grouped);

    let _ = writeln!(out, "}}\n");
}

/// `assign` and `compound_assign`.
///
/// Split deliberately. `assign` is the primitive a language implements: store a
/// value at a place. `compound_assign` is defaulted in terms of it, reading the
/// place, applying the arithmetic role, and writing back — which is only correct
/// because the place was resolved **once**, before either half ran.
fn emit_assignment(
    out: &mut String,
    ops: &[Emitted<'_>],
    grouped: &BTreeMap<String, Vec<(String, String)>>,
) {
    let Some(assign) = ops.iter().find(|e| lazy_lhs(e.op)) else {
        return;
    };
    let role = &assign.role;
    let disc = grouped
        .contains_key(role)
        .then(|| format!("{}Op", type_name(role)));

    let _ = writeln!(
        out,
        "\n    /// Stores `value` at `place`.\n\
        \x20   ///\n\
        \x20   /// `place` arrives with its parts already evaluated, so a subscript\n\
        \x20   /// with a side effect ran exactly once before this was called.\n\
        \x20   fn assign(\n\
        \x20       &mut self,\n\
        \x20       place: Place<'_, Self::Out>,\n\
        \x20       value: Self::Out,\n\
        \x20   ) -> Result<Self::Out> {{\n\
        \x20       let _ = (place, value);\n\
        \x20       Err(Error::unsupported(\"assignment\"))\n\
        \x20   }}\n\n\
        \x20   /// Reads the current value at `place`, for compound assignment.\n\
        \x20   fn place_read(&mut self, place: &Place<'_, Self::Out>) -> Result<Self::Out> {{\n\
        \x20       let _ = place;\n\
        \x20       Err(Error::unsupported(\"reading an assignment target\"))\n\
        \x20   }}"
    );

    if let Some(d) = &disc {
        // Map each compound operator to the arithmetic role it applies.
        let variants: Vec<&(String, String)> = grouped
            .get(role)
            .map(|v| v.iter().collect())
            .unwrap_or_default();

        let _ = writeln!(
            out,
            "\n    /// `+=`, `-=`, and friends.\n\
            \x20   ///\n\
            \x20   /// Defaulted: read the place, apply the operator's arithmetic role,\n\
            \x20   /// store the result. Implement `assign` and `place_read` and this\n\
            \x20   /// works for the whole family.\n\
            \x20   fn compound_assign(\n\
            \x20       &mut self,\n\
            \x20       place: Place<'_, Self::Out>,\n\
            \x20       op: {d},\n\
            \x20       rhs: Self::Out,\n\
            \x20   ) -> Result<Self::Out> {{\n\
            \x20       let current = self.place_read(&place)?;\n\
            \x20       let updated = match op {{"
        );
        for (variant, literal) in &variants {
            let arith = compound_role(literal);
            match arith {
                Some(r) => {
                    let _ = writeln!(
                        out,
                        "            {d}::{variant} => self.{}(current, rhs)?,",
                        ident(r)
                    );
                }
                None => {
                    let _ = writeln!(
                        out,
                        "            // `{literal}` is plain assignment: the old value is discarded.\n\
                        \x20           {d}::{variant} => rhs,"
                    );
                }
            }
        }
        let _ = writeln!(
            out,
            "        }};\n\
            \x20       self.assign(place, updated)\n\
            \x20   }}"
        );
    }
}

/// The arithmetic role a compound assignment applies. `None` for plain `=`.
fn compound_role(literal: &str) -> Option<&'static str> {
    match literal {
        "=" => None,
        "+=" => Some("add"),
        "-=" => Some("sub"),
        "*=" => Some("mul"),
        "/=" => Some("div"),
        "%=" => Some("rem"),
        "<<=" => Some("shl"),
        ">>=" => Some("shr"),
        "&=" => Some("bit_and"),
        "|=" => Some("bit_or"),
        "^=" => Some("bit_xor"),
        _ => None,
    }
}

fn emit_method(
    out: &mut String,
    e: &Emitted<'_>,
    grouped: &BTreeMap<String, Vec<(String, String)>>,
) {
    let m = ident(&e.role);
    let discriminant = grouped
        .contains_key(&e.role)
        .then(|| format!("{}Op", type_name(&e.role)));

    match e.tier.fixity {
        Fixity::Prefix | Fixity::Postfix => {
            let position = if e.tier.fixity == Fixity::Prefix {
                "prefix"
            } else {
                "postfix"
            };
            let arg = match &discriminant {
                Some(d) => format!("op: {d}, "),
                None => String::new(),
            };
            let _ = writeln!(
                out,
                "\n    /// `{}` ({position}).\n\
                \x20   fn {m}(&mut self, {arg}operand: Self::Out) -> Result<Self::Out> {{\n\
                \x20       let _ = operand;\n\
                \x20       Err(Error::unsupported(\"{}\"))\n\
                \x20   }}",
                e.op.literal, e.role
            );
        }
        Fixity::Left | Fixity::Right => {
            let arg = match &discriminant {
                Some(d) => format!("op: {d}, "),
                None => String::new(),
            };

            if lazy_lhs(e.op) {
                // Emitted once by `emit_assignment`: the whole tier shares one
                // place-taking signature, so a per-operator method would be
                // both wrong and duplicated.
            } else if lazy_rhs(e.op) {
                // The lazy signature is the point of the whole OpTree detour:
                // `rhs` arrives unevaluated, so the handler decides.
                let _ = writeln!(
                    out,
                    "\n    /// `{}` — **lazy in its right operand**.\n\
                    \x20   ///\n\
                    \x20   /// `rhs` is unevaluated. Running it is what evaluates it; not\n\
                    \x20   /// running it is what makes this short-circuit.\n\
                    \x20   ///\n\
                    \x20   /// Defaulted to unsupported rather than to truthiness, because\n\
                    \x20   /// only a host with values can answer that. An interpreter gets\n\
                    \x20   /// the standard body from `nh_value_operators!`; a compiler\n\
                    \x20   /// writes its own, emitting a jump.\n\
                    \x20   fn {m}(\n\
                    \x20       &mut self,\n\
                    \x20       lhs: Self::Out,\n\
                    \x20       {arg}rhs: Rc<Expr>,\n\
                    \x20       cx: &mut Ctx,\n\
                    \x20   ) -> Result<Self::Out>\n\
                    \x20   where\n\
                    \x20       Self: Handlers + Sized,\n\
                    \x20   {{\n\
                    \x20       let _ = (&lhs, &rhs, &mut *cx);\n\
                    \x20       Err(Error::unsupported(\"{}\"))\n\
                    \x20   }}",
                    e.op.literal,
                    e.op.literal,
                );
            } else {
                let _ = writeln!(
                    out,
                    "\n    /// `{}`\n\
                    \x20   fn {m}(&mut self, lhs: Self::Out, {arg}rhs: Self::Out) -> Result<Self::Out> {{\n\
                    \x20       let _ = (lhs, rhs);\n\
                    \x20       Err(Error::unsupported(\"{}\"))\n\
                    \x20   }}",
                    e.op.literal, e.role
                );
            }
        }
    }
}

/// `op_info`, `Deferred`, and `eval_tree`.
pub fn emit_driver(out: &mut String, ops: &[Emitted<'_>], atom: &str) {
    let grouped = grouped_roles(ops);

    // --- the table -----------------------------------------------------
    let table_start = out.len();
    let _ = writeln!(
        out,
        "// ---------------------------------------------------------------------------\n\
         // The operator driver\n\
         // ---------------------------------------------------------------------------\n\n\
         /// Precedence, fixity, and associativity per operator rule.\n\
         ///\n\
         /// Precedences come from the resolved table's tier order, so this and\n\
         /// `nh explain` agree by construction.\n\
         pub fn op_info(rule: Rule) -> Option<OpInfo> {{\n\
        \x20   let info = |precedence, fixity, assoc| Some(OpInfo {{ precedence, fixity, assoc }});\n\
        \x20   match rule {{"
    );

    // A literal shared across fixities (`-` as both sub and neg) needs one arm
    // per rule, so prefer the infix reading when both exist: the builder tries
    // prefix only where an operand is expected.
    let mut arms: BTreeMap<&str, &Emitted<'_>> = BTreeMap::new();
    for e in ops {
        arms.entry(&e.rule)
            .and_modify(|existing| {
                if matches!(existing.tier.fixity, Fixity::Prefix | Fixity::Postfix)
                    && matches!(e.tier.fixity, Fixity::Left | Fixity::Right)
                {
                    *existing = e;
                }
            })
            .or_insert(e);
    }

    for (rule, e) in &arms {
        let (fixity, assoc) = match e.tier.fixity {
            Fixity::Left => ("Fixity::Infix", "Assoc::Left"),
            Fixity::Right => ("Fixity::Infix", "Assoc::Right"),
            Fixity::Prefix => ("Fixity::Prefix", "Assoc::Right"),
            Fixity::Postfix => ("Fixity::Postfix", "Assoc::Left"),
        };
        let _ = writeln!(
            out,
            "        Rule::{rule} => info({}, {fixity}, {assoc}),",
            e.precedence
        );
    }

    if arms.is_empty() {
        // No operators: emit the body directly instead of an empty match.
        out.truncate(table_start);
        let _ = writeln!(
            out,
            "/// This grammar declares no operators.\n\
             pub fn op_info(rule: Rule) -> Option<OpInfo> {{\n\
            \x20   let _ = rule;\n\
            \x20   None\n\
             }}\n"
        );
    } else {
        let _ = writeln!(out, "        _ => None,\n    }}\n}}\n");
    }

    // A literal used both as prefix and infix needs its prefix reading too.
    emit_prefix_table(out, ops);

    // --- evaluator -----------------------------------------------------
    //
    // `Deferred` is gone: an operand is an `Rc<Expr>`, which is owned. A lazy
    // operator receives one and decides whether to run it, exactly as before,
    // except that it may now also *keep* it (DESIGN.md §9).
    let _ = writeln!(
        out,
        "/// Evaluates a folded expression.\n\
         ///\n\
         /// The folding already happened, once, when the AST was built — so a\n\
         /// loop that re-tests its condition is not re-folding it every pass.\n\
         pub fn eval_expr<H: Handlers>(\n\
        \x20   host: &mut H,\n\
        \x20   node: &Expr,\n\
        \x20   cx: &mut Ctx,\n\
         ) -> Result<H::Out> {{\n\
        \x20   match node {{\n\
        \x20       Expr::Atom(a) => eval_{atom}(host, a, cx),\n"
    );

    emit_prefix_arms(out, ops, &grouped);
    emit_postfix_arms(out, ops, &grouped);
    emit_infix_arms(out, ops, &grouped);

    let _ = writeln!(
        out,
        "    }}\n\
         }}\n\n\
         impl Eval for Expr {{\n\
        \x20   fn eval<H: Handlers>(&self, host: &mut H, cx: &mut Ctx) -> Result<H::Out> {{\n\
        \x20       eval_expr(host, self, cx)\n\
        \x20   }}\n\
         }}\n"
    );
}

/// The short-circuit bodies, as a macro pasted into an `Operators` impl.
///
/// These cannot be trait defaults any more: they need `Values::truthy`, and a
/// bytecode emitter has no values to inspect. So an interpreter opts in with
/// one line inside its impl, and a compiler writes jump-emitting versions
/// instead. Same roles, same signatures, two shapes.
pub fn emit_value_operators(out: &mut String, ops: &[Emitted<'_>]) {
    let grouped = grouped_roles(ops);
    let lazy: Vec<&Emitted<'_>> = ops
        .iter()
        .filter(|e| lazy_rhs(e.op) && !lazy_lhs(e.op))
        .collect();

    if lazy.is_empty() {
        return;
    }

    let _ = writeln!(
        out,
        "/// The standard short-circuit bodies, for a host that implements\n\
         /// [`Values`].\n\
         ///\n\
         /// Paste it inside your `Operators` impl:\n\
         ///\n\
         /// ```ignore\n\
         /// impl Operators for Interp {{\n\
         ///     nh_value_operators!();\n\
         ///     fn add(&mut self, l: Value, r: Value) -> Result<Value> {{ .. }}\n\
         /// }}\n\
         /// ```\n\
         ///\n\
         /// A bytecode emitter skips this and writes its own, because\n\
         /// short-circuiting compiles to a jump rather than to a decision.\n\
         #[macro_export]\n\
         macro_rules! nh_value_operators {{\n\
        \x20   () => {{"
    );

    let mut seen: Vec<String> = Vec::new();
    for e in &lazy {
        let m = ident(&e.role);
        if seen.contains(&m) {
            continue;
        }
        seen.push(m.clone());

        let arg = discriminant_arg(e, &grouped);
        let arg_ty = if arg.is_empty() {
            String::new()
        } else {
            // `op: CompareOp,` -> spelled through `$crate` inside a macro.
            format!(
                "op: $crate::generated::dispatch::{}Op,\n\x20           ",
                type_name(&e.role)
            )
        };
        let arg_use = if arg.is_empty() { "" } else { "let _ = op;\n\x20           " };

        let _ = writeln!(
            out,
            "        fn {m}(\n\
            \x20           &mut self,\n\
            \x20           lhs: <Self as $crate::generated::dispatch::Semantics>::Out,\n\
            \x20           {arg_ty}rhs: ::std::rc::Rc<$crate::generated::ast::Expr>,\n\
            \x20           cx: &mut ::nh_runtime::Ctx,\n\
            \x20       ) -> ::nh_runtime::Result<\n\
            \x20           <Self as $crate::generated::dispatch::Semantics>::Out,\n\
            \x20       > {{\n\
            \x20           {arg_use}use $crate::generated::dispatch::{{Eval, Values}};\n\
            \x20           if {condition} {{\n\
            \x20               return Ok(lhs);\n\
            \x20           }}\n\
            \x20           rhs.eval(self, cx)\n\
            \x20       }}",
            // `Values` is in scope from the `use` above, so method syntax works.
            condition = short_circuit_when(&e.role),
        );
    }

    let _ = writeln!(out, "    }};\n}}\n");
}

/// Prefix readings of literals that are also infix operators.
fn emit_prefix_table(out: &mut String, ops: &[Emitted<'_>]) {
    let prefixes: Vec<&Emitted<'_>> = ops
        .iter()
        .filter(|e| e.tier.fixity == Fixity::Prefix)
        .collect();

    if prefixes.is_empty() {
        let _ = writeln!(
            out,
            "/// This grammar declares no prefix operators.\n\
             pub fn prefix_info(rule: Rule) -> Option<OpInfo> {{\n\
            \x20   let _ = rule;\n\
            \x20   None\n\
             }}\n"
        );
        return;
    }

    let _ = writeln!(
        out,
        "/// The prefix reading of an operator rule.\n\
         ///\n\
         /// A spelling like `-` is both `sub` and `neg`, and the lowerer emits one\n\
         /// rule for it. The builder consults this where an operand is expected and\n\
         /// [`op_info`] everywhere else.\n\
         pub fn prefix_info(rule: Rule) -> Option<OpInfo> {{\n\
        \x20   let info = |precedence| Some(OpInfo {{\n\
        \x20       precedence,\n\
        \x20       fixity: Fixity::Prefix,\n\
        \x20       assoc: Assoc::Right,\n\
        \x20   }});\n\
        \x20   match rule {{"
    );
    for e in &prefixes {
        let _ = writeln!(out, "        Rule::{} => info({}),", e.rule, e.precedence);
    }
    let _ = writeln!(out, "        _ => None,\n    }}\n}}\n");
}

fn emit_prefix_arms(
    out: &mut String,
    ops: &[Emitted<'_>],
    grouped: &BTreeMap<String, Vec<(String, String)>>,
) {
    let items: Vec<&Emitted<'_>> = ops
        .iter()
        .filter(|e| e.tier.fixity == Fixity::Prefix)
        .collect();
    if items.is_empty() {
        let _ = writeln!(
            out,
            "        Expr::Prefix {{ op, .. }} => Err(Error::runtime(format!(\n\
            \x20           \"`{{op:?}}` is not a prefix operator\"\n\
            \x20       ))),"
        );
        return;
    }

    let _ = writeln!(
        out,
        "        Expr::Prefix {{ op, operand, .. }} => {{\n\
        \x20           let value = eval_expr(host, operand, cx)?;\n\
        \x20           match op {{"
    );
    for e in items {
        let arg = discriminant_arg(e, grouped);
        let _ = writeln!(
            out,
            "                Rule::{} => host.{}({arg}value),",
            e.rule,
            ident(&e.role)
        );
    }
    let _ = writeln!(
        out,
        "                other => Err(Error::runtime(format!(\n\
        \x20                   \"`{{other:?}}` is not a prefix operator\"\n\
        \x20               ))),\n\
        \x20           }}\n\
        \x20       }}"
    );
}

fn emit_postfix_arms(
    out: &mut String,
    ops: &[Emitted<'_>],
    grouped: &BTreeMap<String, Vec<(String, String)>>,
) {
    let items: Vec<&Emitted<'_>> = ops
        .iter()
        .filter(|e| e.tier.fixity == Fixity::Postfix)
        .collect();
    if items.is_empty() {
        let _ = writeln!(
            out,
            "        Expr::Postfix {{ op, .. }} => Err(Error::runtime(format!(\n\
            \x20           \"`{{op:?}}` is not a postfix operator\"\n\
            \x20       ))),"
        );
        return;
    }

    let _ = writeln!(
        out,
        "        Expr::Postfix {{ operand, op, .. }} => {{\n\
        \x20           let value = eval_expr(host, operand, cx)?;\n\
        \x20           match op {{"
    );
    for e in items {
        let arg = discriminant_arg(e, grouped);
        let _ = writeln!(
            out,
            "                Rule::{} => host.{}({arg}value),",
            e.rule,
            ident(&e.role)
        );
    }
    let _ = writeln!(
        out,
        "                other => Err(Error::runtime(format!(\n\
        \x20                   \"`{{other:?}}` is not a postfix operator\"\n\
        \x20               ))),\n\
        \x20           }}\n\
        \x20       }}"
    );
}

fn emit_infix_arms(
    out: &mut String,
    ops: &[Emitted<'_>],
    grouped: &BTreeMap<String, Vec<(String, String)>>,
) {
    let items: Vec<&Emitted<'_>> = ops
        .iter()
        .filter(|e| matches!(e.tier.fixity, Fixity::Left | Fixity::Right))
        .collect();
    if items.is_empty() {
        let _ = writeln!(
            out,
            "        Expr::Infix {{ op, .. }} => Err(Error::runtime(format!(\n\
            \x20           \"`{{op:?}}` is not an infix operator\"\n\
            \x20       ))),"
        );
        return;
    }

    // The left operand is evaluated *inside* each arm, not before the match.
    // An operator lazy in its left operand (assignment) must not have its
    // target evaluated as a value at all.
    let _ = writeln!(
        out,
        "        Expr::Infix {{ lhs, op, rhs, .. }} => match op {{"
    );

    for e in items {
        let arg = discriminant_arg(e, grouped);
        let role = ident(&e.role);

        if lazy_lhs(e.op) {
            let call = if e.op.literal == "=" {
                "host.assign(place, right)".to_string()
            } else {
                format!(
                    "host.compound_assign(place, {}Op::{}, right)",
                    type_name(&e.role),
                    e.variant.clone().unwrap_or_else(|| "Eq".to_string())
                )
            };
            let _ = writeln!(
                out,
                "            &Rule::{} => {{\n\
                \x20               // The target is RESOLVED, not evaluated as a value, and\n\
                \x20               // its parts are computed exactly once.\n\
                \x20               let place = resolve_place(host, lhs, cx)?;\n\
                \x20               let right = eval_expr(host, rhs, cx)?;\n\
                \x20               {call}\n\
                \x20           }}",
                e.rule
            );
        } else if lazy_rhs(e.op) {
            // `rhs` is *not* evaluated here. That is the entire point of the
            // OpTree detour: the handler decides whether it ever runs.
            let _ = writeln!(
                out,
                "            &Rule::{} => {{\n\
                \x20               let left = eval_expr(host, lhs, cx)?;\n\
                \x20               host.{role}(left, {arg}rhs.clone(), cx)\n\
                \x20           }}",
                e.rule
            );
        } else {
            let _ = writeln!(
                out,
                "            &Rule::{} => {{\n\
                \x20               let left = eval_expr(host, lhs, cx)?;\n\
                \x20               let right = eval_expr(host, rhs, cx)?;\n\
                \x20               host.{role}(left, {arg}right)\n\
                \x20           }}",
                e.rule
            );
        }
    }

    let _ = writeln!(
        out,
        "            other => Err(Error::runtime(format!(\n\
        \x20               \"`{{other:?}}` is not an infix operator\"\n\
        \x20           ))),\n\
        \x20       }},"
    );
}

fn discriminant_arg(
    e: &Emitted<'_>,
    grouped: &BTreeMap<String, Vec<(String, String)>>,
) -> String {
    match (&e.variant, grouped.contains_key(&e.role)) {
        (Some(variant), true) => format!("{}Op::{variant}, ", type_name(&e.role)),
        _ => String::new(),
    }
}
