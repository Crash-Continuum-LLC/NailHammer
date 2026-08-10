//! Checking a hand-written handler against the grammar it came from.
//!
//! Most grammar changes are caught by the compiler: add a binding and the arity
//! changes, change a cardinality and the type changes, rename a rule and a new
//! stub appears while the old file is reported as an orphan.
//!
//! That first sentence was too generous. An arity or type change *is* caught by
//! the compiler — but the call it breaks lives in `generated/dispatch.rs`, so
//! the error names a file the author never wrote, and a change that leaves the
//! `Handlers` impl unbuildable surfaces as an unrelated trait bound somewhere
//! else entirely. Being right about which file is wrong is most of the value,
//! so those are reported here too, before rustc gets the chance.
//!
//! Four changes are not caught *usefully*, because parameters are matched
//! positionally and Rust cannot see a parameter's name across a call:
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

/// What a parameter carries, ignoring how the type happens to be spelled.
///
/// A generated stub is generic and says `H::Out`; a hand-written handler is
/// concrete and says `Reg`. Those agree, and comparing the text would say they
/// do not. What cannot differ is the *shape* around the value — whether it
/// arrives once, optionally, repeatedly, or unevaluated — and that is exactly
/// what changes when a binding gains `lazy`, `?` or `*`.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Shape {
    /// `H::Out` / `Reg` — one evaluated value.
    Value,
    /// `Option<H::Out>` — a binding under `?`.
    Optional,
    /// `Vec<H::Out>` — a binding under `*` or `+`.
    Repeated,
    /// `&Shared<Node>` — `lazy`, one node.
    Lazy(String),
    /// `&[Shared<Node>]` — `lazy`, repeated.
    LazyMany(String),
    /// `Option<&Shared<Node>>` — `lazy` under `?`.
    LazyOptional(String),
    /// A token: `&Name` or `&str`.
    Token(String),
    /// Anything this cannot classify, kept verbatim so it still compares.
    Other(String),
}

/// Drops path qualifiers, so `&nh_runtime::Name` and `&Name` are one type.
///
/// A handler may spell a type any way it resolves — imported, qualified, or
/// reached through `crate::` — and none of that is drift. Comparing the text
/// said `&nh_runtime::Name` had been retyped from `&Name`, which is a false
/// alarm on a handler nobody touched, and one of those is worse than the drift
/// this module exists to find.
fn strip_paths(t: &str) -> String {
    let mut out = String::with_capacity(t.len());
    let mut seg = String::new();
    let mut chars = t.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_alphanumeric() || c == '_' {
            seg.push(c);
            continue;
        }
        // `::` after an identifier makes that identifier a path qualifier, and
        // the type is whatever comes next.
        if c == ':' && chars.peek() == Some(&':') {
            chars.next();
            seg.clear();
            continue;
        }
        out.push_str(&seg);
        seg.clear();
        out.push(c);
    }
    out.push_str(&seg);
    out
}

/// Classifies a type as written, in either the generic or the concrete form.
pub fn shape_of(ty: &str) -> Shape {
    let t: String = ty.split_whitespace().collect::<Vec<_>>().join("");
    let t = strip_paths(&t);

    let inner_of = |t: &str, open: &str| -> Option<String> {
        let rest = t.strip_prefix(open)?;
        let rest = rest.strip_suffix('>').or_else(|| rest.strip_suffix(">]"))?;
        Some(rest.to_string())
    };

    if let Some(n) = inner_of(&t, "Option<&Shared<") {
        return Shape::LazyOptional(n.trim_end_matches('>').to_string());
    }
    if let Some(n) = inner_of(&t, "&[Shared<") {
        return Shape::LazyMany(n.to_string());
    }
    if let Some(n) = inner_of(&t, "&Shared<") {
        return Shape::Lazy(n.to_string());
    }
    if t.starts_with("Option<") {
        return Shape::Optional;
    }
    if t.starts_with("Vec<") {
        return Shape::Repeated;
    }
    if t == "&Name" || t == "&str" {
        return Shape::Token(t);
    }
    // `H::Out`, `Self::Out`, `Reg`, `Value` — whatever this language calls the
    // thing an evaluated node produces. One value, however it is spelled.
    if !t.contains('<') && !t.starts_with('&') {
        return Shape::Value;
    }
    Shape::Other(t)
}

/// What a handler file disagrees with its grammar about.
#[derive(Debug, PartialEq, Eq)]
pub enum Drift {
    /// A different number of parameters.
    Arity {
        expected: Vec<String>,
        found: Vec<String>,
    },
    /// The right names, but one of them now carries something else — a binding
    /// gained `lazy`, `?` or `*`, and the handler was not updated.
    Retyped {
        param: String,
        expected: String,
        found: String,
    },
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
        matches!(
            self,
            Drift::Reordered { .. } | Drift::Arity { .. } | Drift::Retyped { .. }
        )
    }

    pub fn message(&self, path: &str) -> String {
        match self {
            Drift::Arity { expected, found } => format!(
                "{path} takes {} parameter(s); the grammar binds {}\n                   grammar:  {}\n  handler:  {}\n\
                 note: the mismatch breaks the generated `Handlers` impl, so the \
                 compiler will blame `generated/dispatch.rs` rather than this file\n\
                 help: match the parameter list to the grammar",
                found.len(),
                expected.len(),
                expected.join(", "),
                found.join(", "),
            ),
            Drift::Retyped { param, expected, found } => format!(
                "{path} takes `{param}` as `{found}`, but the grammar now binds it \
                 as `{expected}`\n\
                 note: `lazy`, `?` and `*` each change what a handler receives, \
                 and so does a token that folds case — this does not guess which\n\
                 note: the compiler will blame `generated/dispatch.rs`, not this \
                 file\n\
                 help: change the parameter's type to match",
            ),
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
    let ps = params(alt);
    let expected: Vec<String> = ps.iter().map(|p| crate::ident(&p.name)).collect();
    let (found, found_tys) = signature_params(source)?;

    // Arity first: it explains any other difference, and reporting a rename on
    // top of it would just be noise about the same edit.
    if expected.len() != found.len() {
        return Some(Drift::Arity { expected, found });
    }

    if expected == found {
        // Same names — so if anything is wrong it is what they carry. This is
        // the case a binding gaining `lazy` produces, and the one that used to
        // escape entirely.
        for (p, got) in ps.iter().zip(&found_tys) {
            let want = shape_of(&p.ty);
            if shape_of(got) != want {
                return Some(Drift::Retyped {
                    param: crate::ident(&p.name),
                    expected: p.ty.clone(),
                    found: got.clone(),
                });
            }
        }
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
        let path = crate::stubs::handler_path(alt);
        if let Some(text) = read(&path) {
            if let Some(d) = check(alt, &text) {
                out.push((alt, d));
            }
        }
    }
    out
}

/// The parameter names **and types** of `pub fn run`, minus the host and `cx`.
///
/// Deliberately forgiving: anything it cannot parse returns `None` and is left
/// alone rather than reported. A false alarm on a handler somebody wrote by
/// hand would be worse than the drift it is looking for.
fn signature_params(source: &str) -> Option<(Vec<String>, Vec<String>)> {
    let at = source.find("pub fn run")?;
    let open = source[at..].find('(')? + at;
    let close = matching_paren(source, open)?;
    let inside = &source[open + 1..close];

    // A parameter list may be commented — the generated stubs are, and a
    // hand-written handler often explains a parameter where it is declared.
    // Left in, a `//` line becomes part of the next parameter's name and the
    // handler is reported as renamed when nothing changed. A false alarm is
    // worse than the drift this looks for, so the comments come out first.
    let inside: String = inside
        .lines()
        .map(|l| match l.find("//") {
            Some(i) => &l[..i],
            None => l,
        })
        .collect::<Vec<_>>()
        .join("\n");

    let mut pairs: Vec<(String, String)> = split_top_level(&inside)
        .into_iter()
        .filter_map(|arg| {
            // Split on the *first* colon only: a type may contain more.
            let (name, ty) = arg.split_once(':')?;
            let name = name.trim().trim_start_matches("mut ").trim();
            if name.is_empty() || name == "self" || name == "&self" {
                return None;
            }
            Some((name.to_string(), ty.trim().to_string()))
        })
        .collect();

    // The first parameter is the host and the last is `cx`; neither comes from
    // a binding.
    if pairs.len() < 2 {
        return None;
    }
    pairs.pop();
    pairs.remove(0);

    Some((
        pairs
            .iter()
            .map(|(n, _)| n.trim_start_matches('_').to_string())
            .collect(),
        pairs.iter().map(|(_, t)| t.clone()).collect(),
    ))
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
        assert_eq!(signature_params(src).unwrap().0, vec!["key", "value"]);
    }

    /// A type containing a comma must not be split in half.
    #[test]
    fn generic_types_do_not_confuse_the_split() {
        let src = "pub fn run(h: &mut I, a: Option<&str>, b: &[Shared<Line>], c: HashMap<K, V>, cx: &mut Ctx)";
        assert_eq!(signature_params(src).unwrap().0, vec!["a", "b", "c"]);
    }

    /// Multi-line signatures are the common case for wide rules.
    #[test]
    fn a_wrapped_signature_reads_back() {
        let src = "pub fn run(\n    host: &mut Interp,\n    var: &Name,\n    body: &[Shared<Line>],\n    cx: &mut Ctx,\n) -> Result<Value> {";
        assert_eq!(signature_params(src).unwrap().0, vec!["var", "body"]);
    }

    /// How a type is *reached* is not what it is. A handler that qualifies its
    /// path has not been retyped, and saying it has is a false alarm on code
    /// nobody touched.
    #[test]
    fn a_path_qualifier_is_not_a_different_type() {
        assert_eq!(shape_of("&nh_runtime::Name"), shape_of("&Name"));
        assert_eq!(
            shape_of("&Shared<crate::generated::ast::Block>"),
            shape_of("&Shared<Block>")
        );
        assert_eq!(shape_of("H::Out"), shape_of("Value"));
        // And a real difference still is one.
        assert_ne!(shape_of("Option<H::Out>"), shape_of("H::Out"));
        assert_ne!(shape_of("&Shared<Block>"), shape_of("&[Shared<Block>]"));
    }

    /// An unused parameter is conventionally `_name`; that is not drift.
    #[test]
    fn a_leading_underscore_is_not_a_different_name() {
        let src = "pub fn run(_host: &mut Interp, _key: &str, value: Value, _cx: &mut Ctx)";
        assert_eq!(signature_params(src).unwrap().0, vec!["key", "value"]);
    }

    /// Adding or removing a binding is an arity error. The compiler's message
    /// for that names a parameter and its type — but it names it at the *call*,
    /// in `generated/dispatch.rs`, so the author is sent to a file they never
    /// wrote. Reported here instead, pointing at the handler.
    /// A commented parameter list is not drifted. The generated stubs carry
    /// doc comments, and a hand-written handler often explains a parameter
    /// where it is declared — swallowing one into the next name reported a
    /// rename that never happened.
    #[test]
    fn comments_in_a_parameter_list_are_ignored() {
        let src = "pub fn run(\n    host: &mut Interp,\n    name: &Name,\n    // why this is here\n    ret: Option<Reg>,\n    cx: &mut Ctx,\n)";
        let (names, tys) = signature_params(src).unwrap();
        assert_eq!(names, vec!["name", "ret"]);
        assert_eq!(tys, vec!["&Name", "Option<Reg>"]);
    }

    #[test]
    fn a_different_parameter_count_is_reported() {
        let src = "pub fn run(h: &mut I, key: &str, cx: &mut Ctx)";
        let (names, tys) = signature_params(src).unwrap();
        assert_eq!(names, vec!["key"]);
        assert_eq!(tys, vec!["&str"]);
        // `check` needs a LoweredAlternative, so the arity rule itself is
        // asserted in `crates/nh-codegen/tests/generate.rs`.
    }

    #[test]
    fn nothing_parseable_means_nothing_reported() {
        assert_eq!(signature_params("// somebody rewrote this entirely"), None);
    }
}
