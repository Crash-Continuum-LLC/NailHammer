//! Checking a hand-written handler against the grammar it came from.
//!
//! Most grammar changes are caught by the compiler: add a binding and the arity
//! changes, change a cardinality and the type changes, rename a rule and a new
//! stub appears while the old file is reported as an orphan.
//!
//! Two changes are **not** caught, because parameters are matched positionally
//! and Rust cannot see a parameter's name across a call:
//!
//! * **Renaming a binding.** The handler keeps the old parameter name and
//!   silently receives the right value under the wrong name. Harmless, but the
//!   handler now lies about what it is reading.
//! * **Reordering two bindings of the same type.** The handler receives them
//!   swapped, with no error and no warning. This is a real defect, and it is
//!   the reason this module exists.
//!
//! The view-based handlers of M2–M6 could not have this problem — `view.key()`
//! looked a child up by name. Parameters are a better interface (DESIGN.md
//! §5.4) but they gave that property up, so it is checked here instead.

use nh_lower::{Lowered, LoweredAlternative};

use crate::params::params;

/// What a handler file disagrees with its grammar about.
#[derive(Debug, PartialEq, Eq)]
pub enum Drift {
    /// The same names, in a different order. The handler is now wrong.
    Reordered {
        expected: Vec<String>,
        found: Vec<String>,
    },
    /// Different names. Usually a rename; the handler still works but no longer
    /// says what it is reading.
    Renamed {
        expected: Vec<String>,
        found: Vec<String>,
    },
}

impl Drift {
    /// Whether this is a defect rather than a cosmetic mismatch.
    pub fn is_error(&self) -> bool {
        matches!(self, Drift::Reordered { .. })
    }

    pub fn message(&self, path: &str) -> String {
        match self {
            Drift::Reordered { expected, found } => format!(
                "{path} takes its parameters in a different order than the grammar \
                 binds them\n  grammar:  {}\n  handler:  {}\n\
                 note: parameters are positional, so this handler now receives them \
                 swapped\nhelp: reorder the parameters to match the grammar",
                expected.join(", "),
                found.join(", "),
            ),
            Drift::Renamed { expected, found } => format!(
                "{path} names its parameters differently than the grammar binds them\
                 \n  grammar:  {}\n  handler:  {}\n\
                 help: rename the parameters to match, so the handler says what it reads",
                expected.join(", "),
                found.join(", "),
            ),
        }
    }
}

/// Compares one handler's `run` signature against its grammar alternative.
///
/// `source` is the handler file's text. Returns `None` when they agree, or when
/// the file has no signature this can read — a handler somebody rewrote by hand
/// into a different shape is theirs, not something to complain about.
pub fn check(alt: &LoweredAlternative, source: &str) -> Option<Drift> {
    let expected: Vec<String> = params(alt)
        .iter()
        .map(|p| crate::ident(&p.name))
        .collect();
    let found = signature_params(source)?;

    if expected == found {
        return None;
    }

    // A different number of parameters is an arity error, and the compiler's
    // message for it names the parameter and its type. Saying something vaguer
    // first would be noise.
    if expected.len() != found.len() {
        return None;
    }

    let mut a = expected.clone();
    let mut b = found.clone();
    a.sort();
    b.sort();

    Some(if a == b {
        Drift::Reordered { expected, found }
    } else {
        Drift::Renamed { expected, found }
    })
}

/// Every handler whose file disagrees with the grammar.
pub fn check_all(
    lowered: &Lowered,
    read: impl Fn(&str) -> Option<String>,
) -> Vec<(&LoweredAlternative, Drift)> {
    let mut out = Vec::new();
    for alt in &lowered.alternatives {
        let path = format!("handlers/{}.rs", alt.pest_rule);
        if let Some(text) = read(&path) {
            if let Some(d) = check(alt, &text) {
                out.push((alt, d));
            }
        }
    }
    out
}

/// The parameter names of `pub fn run`, minus the host and `cx`.
///
/// Deliberately forgiving: anything it cannot parse returns `None` and is left
/// alone rather than reported. A false alarm on a handler somebody wrote by
/// hand would be worse than the drift it is looking for.
fn signature_params(source: &str) -> Option<Vec<String>> {
    let at = source.find("pub fn run")?;
    let open = source[at..].find('(')? + at;
    let close = matching_paren(source, open)?;
    let inside = &source[open + 1..close];

    let mut names: Vec<String> = split_top_level(inside)
        .into_iter()
        .filter_map(|arg| {
            let name = arg.split(':').next()?.trim();
            let name = name.trim_start_matches("mut ").trim();
            if name.is_empty() || name == "self" || name == "&self" {
                None
            } else {
                Some(name.to_string())
            }
        })
        .collect();

    // The first parameter is the host and the last is `cx`; neither comes from
    // a binding.
    if names.len() < 2 {
        return None;
    }
    names.pop();
    names.remove(0);

    Some(names.iter().map(|n| n.trim_start_matches('_').to_string()).collect())
}

fn matching_paren(s: &str, open: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0usize;
    for (i, b) in bytes.iter().enumerate().skip(open) {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Splits on commas that are not inside brackets.
///
/// A naive split would cut `Option<&str>` in half the moment a type contains a
/// comma, which `HashMap<K, V>` and `Deferred<'_, '_>` both do.
fn split_top_level(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;

    for (i, c) in s.char_indices() {
        match c {
            '<' | '(' | '[' => depth += 1,
            '>' | ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                out.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    if !s[start..].trim().is_empty() {
        out.push(&s[start..]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_signature_reads_back() {
        let src = "pub fn run(host: &mut Interp, key: &str, value: Value, cx: &mut Ctx) -> Result<Value> {";
        assert_eq!(signature_params(src).unwrap(), vec!["key", "value"]);
    }

    /// A type containing a comma must not be split in half.
    #[test]
    fn generic_types_do_not_confuse_the_split() {
        let src = "pub fn run(h: &mut I, a: Option<&str>, b: &[Shared<Line>], c: HashMap<K, V>, cx: &mut Ctx)";
        assert_eq!(signature_params(src).unwrap(), vec!["a", "b", "c"]);
    }

    /// Multi-line signatures are the common case for wide rules.
    #[test]
    fn a_wrapped_signature_reads_back() {
        let src = "pub fn run(\n    host: &mut Interp,\n    var: &Name,\n    body: &[Shared<Line>],\n    cx: &mut Ctx,\n) -> Result<Value> {";
        assert_eq!(signature_params(src).unwrap(), vec!["var", "body"]);
    }

    /// An unused parameter is conventionally `_name`; that is not drift.
    #[test]
    fn a_leading_underscore_is_not_a_different_name() {
        let src = "pub fn run(_host: &mut Interp, _key: &str, value: Value, _cx: &mut Ctx)";
        assert_eq!(signature_params(src).unwrap(), vec!["key", "value"]);
    }

    /// Adding or removing a binding is an arity error. The compiler's message
    /// for that is better than anything this could say, so this says nothing.
    #[test]
    fn a_different_parameter_count_is_left_to_the_compiler() {
        let src = "pub fn run(h: &mut I, key: &str, cx: &mut Ctx)";
        let names = signature_params(src).unwrap();
        assert_eq!(names, vec!["key"]);
        // `check` needs a LoweredAlternative, so the count rule is asserted in
        // `crates/nh-codegen/tests/generate.rs` where one is available.
    }

    #[test]
    fn nothing_parseable_means_nothing_reported() {
        assert_eq!(signature_params("// somebody rewrote this entirely"), None);
    }
}
