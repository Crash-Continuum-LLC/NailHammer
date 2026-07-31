//! Operator tables for NailHammer.
//!
//! Resolves a grammar's `use operators::<preset>` and `precedence` blocks into
//! a single [`OperatorTable`]: an ordered list of tiers, lowest precedence
//! first, each carrying its operators, fixity, semantic role, and laziness.
//!
//! This crate holds the *table*. The expression driver that folds a parse using
//! it — `OpTree`, `Thunk`, the `Operators` trait — is M3. The table is needed
//! earlier because M1 emits the flat `expr` rule and its operator alternations
//! straight from it.

pub mod presets;
pub mod roles;

use nh_syntax::ast::{Ast, Direction, Fixity, OpRef, PrecEntry, PrecedenceBlock};
use nh_syntax::{Diagnostic, Errors, SourceMap, Span, Spanned};

/// One operator in a resolved table.
#[derive(Clone, Debug)]
pub struct Operator {
    pub literal: String,
    /// Identifier-shaped (`word "AND"`), so it needs a boundary guard and
    /// auto-reservation (DESIGN.md §6.5).
    pub word: bool,
    pub role: String,
    /// Operand positions left unevaluated.
    pub lazy: Vec<String>,
    pub span: Option<Span>,
}

/// One precedence level. Tier 0 binds loosest.
#[derive(Clone, Debug)]
pub struct Tier {
    pub fixity: Fixity,
    pub operators: Vec<Operator>,
    /// Set when the whole tier shares one role, which makes the generated trait
    /// method take a discriminant instead of producing one method per operator.
    pub grouped_role: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct OperatorTable {
    /// Lowest precedence first.
    pub tiers: Vec<Tier>,
    /// The rule the operator driver folds over (`atom NAME;`).
    pub atom_rule: Option<String>,
    /// Preset this table started from, for `nh explain`.
    pub preset: Option<String>,
    /// Non-fatal diagnostics produced while resolving.
    ///
    /// Resolving can notice things worth saying that do not stop it — a bare
    /// `precedence` block silently discarding a preset, for one. Returning them
    /// means a caller can surface them; before this they were computed and
    /// dropped on the success path, so the single most destructive thing this
    /// crate can do to a grammar happened without a word. `nh-lower` had the
    /// same defect and fixed it the same way (DESIGN.md §11).
    pub diagnostics: Vec<Diagnostic>,
}

impl OperatorTable {
    pub fn is_empty(&self) -> bool {
        self.tiers.iter().all(|t| t.operators.is_empty())
    }

    pub fn operators(&self) -> impl Iterator<Item = (&Tier, &Operator)> {
        self.tiers
            .iter()
            .flat_map(|t| t.operators.iter().map(move |o| (t, o)))
    }

    /// The precedence number to *show* a user for the tier at `index`.
    ///
    /// Tiers are stored loosest-first, so the index is the binding power the
    /// driver folds with — but it reads backwards in a printed table, where the
    /// familiar convention (a C precedence chart) numbers the tightest tier 1.
    /// So display is the inverse of storage.
    ///
    /// This exists because `nh explain` and `nh trace` computed it separately
    /// and disagreed: explain printed `1` for the tightest tier while trace
    /// printed the raw index, so the same operator was "precedence 1" in one
    /// command and "precedence 2" in the other. One definition, two callers.
    pub fn display_precedence(&self, index: usize) -> usize {
        self.tiers.len().saturating_sub(index)
    }

    pub fn has_fixity(&self, f: Fixity) -> bool {
        self.tiers
            .iter()
            .any(|t| t.fixity == f && !t.operators.is_empty())
    }

    /// Operators of a given fixity, **sorted longest literal first**.
    ///
    /// This ordering is not cosmetic. In a PEG alternation, `"<" | "<="` makes
    /// `<=` unreachable: the first alternative wins on the `<` and the `=` is
    /// left for the next token. Sorting by descending length yields
    /// maximal munch, which is also what C does for `a+++b` (`++ +`).
    /// DESIGN.md §5.2 requires this, and requires that it apply *only* to
    /// alternations NailHammer synthesises.
    pub fn sorted_by_fixity(&self, f: Fixity) -> Vec<&Operator> {
        let mut ops: Vec<&Operator> = self
            .tiers
            .iter()
            .filter(|t| t.fixity == f)
            .flat_map(|t| t.operators.iter())
            .collect();
        ops.sort_by(|a, b| {
            b.literal
                .len()
                .cmp(&a.literal.len())
                .then_with(|| a.literal.cmp(&b.literal))
        });
        ops
    }

    fn find_tier_of(&self, literal: &str) -> Option<usize> {
        self.tiers
            .iter()
            .position(|t| t.operators.iter().any(|o| o.literal == literal))
    }
}

/// Resolves a grammar's operator declarations into one table.
pub fn resolve(ast: &Ast, sm: &mut SourceMap) -> Result<OperatorTable, Errors> {
    let mut diagnostics = Vec::new();
    let mut table = OperatorTable::default();

    // 1. Start from a preset, if one was named.
    if ast.uses.len() > 1 {
        diagnostics.push(
            Diagnostic::error("more than one `use operators::` declaration")
                .at(ast.uses[1].span)
                .note("first declared here", Some(ast.uses[0].span))
                .help("a grammar has exactly one operator table; use `precedence override` to adjust it"),
        );
    }

    if let Some(u) = ast.uses.first() {
        match presets::source(&u.preset.value) {
            Some(src) => {
                table.preset = Some(u.preset.value.clone());
                match nh_syntax::parse_source(sm, format!("<preset {}>", u.preset.value), src) {
                    Ok(preset_ast) => {
                        for block in &preset_ast.precedence {
                            apply_block(&mut table, block, &mut diagnostics, true);
                        }
                    }
                    Err(e) => diagnostics.extend(e.0),
                }
            }
            None => diagnostics.push(
                Diagnostic::error(format!("unknown operator preset `{}`", u.preset.value))
                    .at(u.preset.span)
                    .help(format!("available presets: {}", presets::NAMES.join(", "))),
            ),
        }
    }

    // 2. Apply the grammar's own blocks, in source order.
    for block in &ast.precedence {
        if !block.is_override && table.preset.is_some() && !table.is_empty() {
            diagnostics.push(
                Diagnostic::warning(
                    "a bare `precedence` block replaces the preset table entirely",
                )
                .at(block.span)
                .help("write `precedence override { .. }` to adjust the preset instead"),
            );
            table.tiers.clear();
        }
        apply_block(&mut table, block, &mut diagnostics, false);
    }

    validate_roles(&table, &mut diagnostics);

    if diagnostics.iter().any(|d| d.severity == nh_syntax::Severity::Error) {
        Err(Errors(diagnostics))
    } else {
        // Carried rather than discarded. Everything left here is a warning, and
        // a warning nobody sees is the same as no warning at all.
        table.diagnostics = diagnostics;
        Ok(table)
    }
}

/// A role must be bound at exactly one fixity.
///
/// A role names one operation with one signature: an infix `compare` takes two
/// operands, a prefix one takes a single operand. Binding both emits two trait
/// methods of the same name, and the generated code does not compile — with an
/// error pointing at generated Rust rather than at the grammar that caused it.
///
/// Operators that *look* like a shared spelling across fixities are already
/// distinct roles: `-` is `sub` infix and `neg` prefix.
fn validate_roles(table: &OperatorTable, diagnostics: &mut Vec<Diagnostic>) {
    use std::collections::HashMap;

    let mut seen: HashMap<&str, (Fixity, Option<Span>)> = HashMap::new();

    for tier in &table.tiers {
        let Some(first) = tier.operators.first() else {
            continue;
        };
        for op in &tier.operators {
            let role = tier.grouped_role.as_deref().unwrap_or(&op.role);
            let entry = seen.entry(role).or_insert((tier.fixity, first.span));

            if entry.0 == tier.fixity {
                continue;
            }

            let d = Diagnostic::error(format!(
                "role `{role}` is bound at two different fixities ({} and {})",
                describe_fixity(entry.0),
                describe_fixity(tier.fixity)
            ))
            .note("first bound here", entry.1)
            .help(
                "a role names one operation with one signature; use distinct \
                 roles, as `-` uses `sub` when infix and `neg` when prefix",
            );
            diagnostics.push(match op.span {
                Some(s) => d.at(s),
                None => d,
            });
            break;
        }
    }
}

fn describe_fixity(f: Fixity) -> &'static str {
    match f {
        Fixity::Left => "infix, left-associative",
        Fixity::Right => "infix, right-associative",
        Fixity::Prefix => "prefix",
        Fixity::Postfix => "postfix",
    }
}

fn apply_block(
    table: &mut OperatorTable,
    block: &PrecedenceBlock,
    diagnostics: &mut Vec<Diagnostic>,
    _from_preset: bool,
) {
    for entry in &block.entries {
        match entry {
            PrecEntry::Atom { rule, .. } => table.atom_rule = Some(rule.value.clone()),

            PrecEntry::Remove { ops, span } => {
                for op in ops {
                    match table.find_tier_of(&op.literal.value) {
                        Some(i) => {
                            table.tiers[i]
                                .operators
                                .retain(|o| o.literal != op.literal.value);
                        }
                        None => diagnostics.push(
                            Diagnostic::error(format!(
                                "cannot remove `{}`: not in the table",
                                op.literal.value
                            ))
                            .at(op.span)
                            .help("check the spelling, or run `nh explain` to see the table"),
                        ),
                    }
                }
                let _ = span;
                table.tiers.retain(|t| !t.operators.is_empty());
            }

            PrecEntry::Op(op_entry) => {
                let fixity = op_entry.fixity.value;
                let prefix = matches!(fixity, Fixity::Prefix);

                let mut operators = Vec::new();
                for op in &op_entry.ops {
                    match resolve_role(op, op_entry.role.as_ref(), prefix) {
                        Ok(role) => {
                            let lazy = if op_entry.lazy.is_empty() {
                                roles::default_lazy(&role)
                                    .iter()
                                    .map(|s| s.to_string())
                                    .collect()
                            } else {
                                op_entry.lazy.iter().map(|l| l.value.clone()).collect()
                            };
                            operators.push(Operator {
                                literal: op.literal.value.clone(),
                                word: op.word,
                                role,
                                lazy,
                                span: Some(op.span),
                            });
                        }
                        Err(d) => diagnostics.push(d),
                    }
                }

                if operators.is_empty() {
                    continue;
                }

                let tier = Tier {
                    fixity,
                    operators,
                    grouped_role: op_entry.role.as_ref().map(|r| r.value.clone()),
                };

                match &op_entry.placement {
                    None => table.tiers.push(tier),
                    Some(p) => match table.find_tier_of(&p.anchor.value) {
                        Some(i) => {
                            // Tier 0 binds loosest, so "above" (tighter) means a
                            // higher index.
                            let at = match p.direction {
                                Direction::Above => i + 1,
                                Direction::Below => i,
                            };
                            table.tiers.insert(at, tier);
                        }
                        None => diagnostics.push(
                            Diagnostic::error(format!(
                                "unknown anchor operator `{}`",
                                p.anchor.value
                            ))
                            .at(p.anchor.span)
                            .help("`above`/`below` must name an operator already in the table"),
                        ),
                    },
                }
            }
        }
    }
}

fn resolve_role(
    op: &OpRef,
    tier_role: Option<&Spanned<String>>,
    prefix: bool,
) -> Result<String, Diagnostic> {
    if let Some(r) = tier_role {
        return Ok(r.value.clone());
    }

    let found = if op.word {
        roles::role_for_word(&op.literal.value, prefix)
    } else {
        roles::role_for(&op.literal.value, prefix)
    };

    found.map(str::to_string).ok_or_else(|| {
        Diagnostic::error(format!(
            "no built-in role for operator `{}`",
            op.literal.value
        ))
        .at(op.span)
        .help("add an explicit binding, e.g. `-> pipe`, or bind the whole tier with `-> role`")
    })
}

// ---------------------------------------------------------------------------
// explain
// ---------------------------------------------------------------------------

/// Renders the resolved table as the listing from DESIGN.md §5.2.
///
/// Precedence lives in a generated table rather than in the shape of the
/// `.pest`, so this is how it stays inspectable.
pub fn explain(table: &OperatorTable) -> String {
    let mut out = String::new();

    if let Some(p) = &table.preset {
        out.push_str(&format!("preset: operators::{p}\n\n"));
    }
    if table.tiers.is_empty() {
        out.push_str("(empty table)\n");
        return out;
    }

    // Measured over the joined list, since that is what gets padded.
    let width = table
        .tiers
        .iter()
        .map(|t| {
            t.operators
                .iter()
                .map(|o| o.literal.len() + 1)
                .sum::<usize>()
                .saturating_sub(1)
        })
        .max()
        .unwrap_or(4)
        .max(4);

    // Highest precedence number = loosest, matching how people read a
    // precedence table top-down. `display_precedence` owns that inversion so
    // `nh trace` prints the same number for the same tier.
    for (i, tier) in table.tiers.iter().enumerate() {
        if tier.operators.is_empty() {
            continue;
        }
        let ops = tier
            .operators
            .iter()
            .map(|o| o.literal.clone())
            .collect::<Vec<_>>()
            .join(" ");

        let fixity = match tier.fixity {
            Fixity::Left => "left",
            Fixity::Right => "right",
            Fixity::Prefix => "prefix",
            Fixity::Postfix => "postfix",
        };

        let mut line = format!(
            "{:>3}  {:<width$}  {:<7}",
            table.display_precedence(i),
            ops,
            fixity
        );

        let lazy: Vec<&str> = tier
            .operators
            .iter()
            .flat_map(|o| o.lazy.iter())
            .map(String::as_str)
            .collect();
        if !lazy.is_empty() {
            let mut uniq: Vec<&str> = lazy;
            uniq.sort_unstable();
            uniq.dedup();
            line.push_str(&format!(" lazy({})", uniq.join(", ")));
        }

        if let Some(role) = &tier.grouped_role {
            line.push_str(&format!("  -> {role}"));
        } else {
            let roles: Vec<&str> = {
                let mut r: Vec<&str> = tier.operators.iter().map(|o| o.role.as_str()).collect();
                r.dedup();
                r
            };
            line.push_str(&format!("  -> {}", roles.join(", ")));
        }

        out.push_str(line.trim_end());
        out.push('\n');
    }

    match &table.atom_rule {
        Some(rule) => out.push_str(&format!("\natom: `{rule}`\n")),
        None => out.push_str("\natom: (none declared)\n"),
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table_from(source: &str) -> Result<OperatorTable, String> {
        let mut sm = SourceMap::new();
        let ast = nh_syntax::parse_source(&mut sm, "<test>", source)
            .unwrap_or_else(|e| panic!("{}", e.render(&sm)));
        resolve(&ast, &mut sm).map_err(|e| e.render(&sm))
    }

    /// A role names one operation with one signature. Binding it at two
    /// fixities emits two trait methods of the same name, and the generated
    /// code does not compile — with an error pointing at generated Rust rather
    /// than at the grammar.
    #[test]
    fn a_role_cannot_be_bound_at_two_fixities() {
        let err = table_from(
            "grammar T;\nprecedence {\n  left \"==\" -> compare;\n  \
             prefix \"!\" -> compare;\n  atom a;\n}\n",
        )
        .expect_err("must be rejected");
        assert!(err.contains("two different fixities"), "{err}");
        assert!(err.contains("`compare`"), "{err}");
    }

    /// Two tiers sharing a role at the *same* fixity is fine, and is how an
    /// imported table extends another: the discriminant unions both.
    #[test]
    fn two_tiers_may_share_a_role_at_one_fixity() {
        let table = table_from(
            "grammar T;\nprecedence {\n  left \"==\" | \"!=\" -> compare;\n  \
             left \"<\" | \">\" -> compare;\n  atom a;\n}\n",
        )
        .expect("same fixity is fine");
        assert_eq!(table.operators().count(), 4);
    }

    /// The presets must satisfy their own rule. `-` really is two roles.
    #[test]
    fn every_preset_resolves_cleanly() {
        for name in presets::NAMES {
            let source = format!("grammar T;\nuse operators::{name};\n");
            table_from(&source).unwrap_or_else(|e| panic!("preset `{name}`:\n{e}"));
        }
    }

    /// A bare `precedence` block discards the preset, and **says so**.
    ///
    /// The warning was constructed correctly all along and then thrown away:
    /// `resolve` returned `Ok(table)` and dropped every non-error diagnostic,
    /// so the most destructive thing this crate does to a grammar happened in
    /// silence. A user saw 35 operators become 5 with a clean `ok:`.
    #[test]
    fn a_bare_block_warns_that_it_discarded_the_preset() {
        let table = table_from(
            "grammar T;\nuse operators::c_style;\nprecedence {\n  \
             left \"+\";\n  atom a;\n}\n",
        )
        .expect("replacing a preset is legal, just loud");

        assert_eq!(table.operators().count(), 1, "the preset really is gone");
        assert!(
            table
                .diagnostics
                .iter()
                .any(|d| d.message.contains("replaces the preset")),
            "the warning must survive the success path: {:?}",
            table.diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    /// `override` is the spelling that adjusts, and it must stay quiet —
    /// a warning that fires on the correct spelling is one people learn to
    /// ignore, and then it stops protecting anyone.
    #[test]
    fn override_adjusts_the_preset_without_warning() {
        let table = table_from(
            "grammar T;\nuse operators::c_style;\nprecedence override {\n  \
             right \"**\" above \"*\" -> pow;\n}\n",
        )
        .expect("override is the supported spelling");

        assert!(
            table.operators().count() > 30,
            "the preset survives: {}",
            table.operators().count()
        );
        assert!(
            table.diagnostics.is_empty(),
            "no warning for the right spelling: {:?}",
            table.diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    /// Storage is loosest-first; display is tightest-first. The inversion is
    /// the whole reason this function exists rather than being inlined.
    #[test]
    fn display_precedence_inverts_the_stored_index() {
        let table = table_from(
            "grammar T;\nprecedence {\n  left \"+\";\n  left \"*\";\n  \
             right \"^\" -> pow;\n  atom a;\n}\n",
        )
        .expect("three tiers");

        assert_eq!(table.tiers.len(), 3);
        assert_eq!(table.display_precedence(0), 3, "loosest prints highest");
        assert_eq!(table.display_precedence(2), 1, "tightest prints 1");
    }

    /// `nh explain` must print what `display_precedence` says, because
    /// `nh trace` prints the same function's output and the two commands
    /// previously disagreed about the same operator.
    #[test]
    fn explain_prints_the_display_number() {
        let table = table_from(
            "grammar T;\nprecedence {\n  left \"+\";\n  left \"*\";\n  \
             right \"^\" -> pow;\n  atom a;\n}\n",
        )
        .expect("three tiers");

        let out = explain(&table);
        let line_for = |op: &str| {
            out.lines()
                .find(|l| l.contains(op))
                .unwrap_or_else(|| panic!("no line for `{op}` in:\n{out}"))
                .split_whitespace()
                .next()
                .unwrap()
                .parse::<usize>()
                .unwrap()
        };

        assert_eq!(line_for("+"), table.display_precedence(0));
        assert_eq!(line_for("^"), table.display_precedence(2));
        assert!(line_for("+") > line_for("^"), "looser prints higher");
    }
}
