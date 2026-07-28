//! Determinism analysis (DESIGN.md §5.1).
//!
//! This is the layer the project exists for. A PEG will happily accept a
//! grammar that silently means something other than it looks like: `a | ab`
//! never matches `ab`, `(x?)*` loops forever, and left recursion is simply
//! forbidden. None of that is visible in the grammar text.
//!
//! **Every lint here is conservative.** A hazard that is undecidable in general
//! gets reported only in the cases where it is *certain* — because a
//! determinism warning that cries wolf is a warning people learn to ignore,
//! and then it protects nobody. Where a check would need to guess, it stays
//! silent instead.
//!
//! Anything genuinely intentional can be silenced per-rule with
//! `allow <lint> in <rule>;`.

mod first;
mod lints;

use std::collections::HashSet;

use nh_syntax::{Ast, Diagnostic, Errors, Severity};

pub use lints::LINTS;

/// Runs every pass. Errors and warnings come back together, so a caller can
/// decide whether warnings are fatal.
///
/// `operator_atom` is the rule the resolved operator table folds over. It must
/// be passed in rather than read from the AST: with `use operators::<preset>`
/// the `atom` entry lives in the preset, so `atom` looks unreferenced from the
/// grammar text alone. Reporting it would be a false positive on every grammar
/// that uses a preset — and one bad warning is enough to teach people to ignore
/// the rest.
pub fn analyse(ast: &Ast, operator_atom: Option<&str>) -> Vec<Diagnostic> {
    let allowed = Allowed::from(ast);
    let mut out = Vec::new();

    lints::left_recursion(ast, &allowed, &mut out);
    lints::nullable_repetition(ast, &allowed, &mut out);
    lints::shadowed_alternatives(ast, &allowed, &mut out);
    lints::unreachable_alternatives(ast, &allowed, &mut out);
    lints::duplicate_bindings(ast, &allowed, &mut out);
    lints::unused_definitions(ast, operator_atom, &allowed, &mut out);
    lints::recover_sync(ast, &allowed, &mut out);
    lints::silent_binding(ast, &allowed, &mut out);
    lints::unknown_allows(ast, &mut out);

    out
}

/// Convenience wrapper: fails if there are errors, or if `deny_warnings` and
/// there are any warnings.
pub fn check(
    ast: &Ast,
    operator_atom: Option<&str>,
    deny_warnings: bool,
) -> Result<Vec<Diagnostic>, Errors> {
    let diagnostics = analyse(ast, operator_atom);
    let fatal = diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error || (deny_warnings && d.severity == Severity::Warning));

    if fatal {
        Err(Errors(diagnostics))
    } else {
        Ok(diagnostics)
    }
}

/// The set of `allow <lint> in <rule>;` declarations.
pub struct Allowed {
    entries: HashSet<(String, String)>,
}

impl Allowed {
    fn from(ast: &Ast) -> Self {
        Allowed {
            entries: ast
                .allows
                .iter()
                .map(|a| (a.lint.value.clone(), a.rule.value.clone()))
                .collect(),
        }
    }

    pub fn is_allowed(&self, lint: &str, rule: &str) -> bool {
        self.entries.contains(&(lint.to_string(), rule.to_string()))
    }
}

/// Formats a diagnostic with its lint name, so the message says how to silence
/// it without the user hunting for the name.
pub(crate) fn lint(
    severity: Severity,
    name: &'static str,
    rule: &str,
    message: impl Into<String>,
) -> Diagnostic {
    let d = match severity {
        Severity::Error => Diagnostic::error(message),
        Severity::Warning => Diagnostic::warning(message),
    };
    d.code(name)
        .note(format!("lint: `{name}`"), None)
        .help(format!("if this is intentional, write `allow {name} in {rule};`"))
}
