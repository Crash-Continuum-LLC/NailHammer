//! Emitting valid pest surface syntax.
//!
//! Small, but shared: both the expression emitter and the continuation-class
//! derivation need to write literals and character ranges, and they must agree
//! byte for byte or the generated guards will not match the generated tokens.

/// A pest string literal, e.g. `"let"` or `^"AND"` when folding.
///
/// Folding is only emitted for literals that actually contain a letter.
/// `^"="` and `^";"` are no-ops that clutter the generated grammar and imply a
/// case-sensitivity that punctuation does not have.
pub fn string(value: &str, case_insensitive: bool) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    if case_insensitive && value.chars().any(|c| c.is_ascii_alphabetic()) {
        out.push('^');
    }
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A pest character range, e.g. `'a'..'z'`.
///
/// Pest ranges use single quotes and single characters, unlike `.nh`, which
/// spells them `"a".."z"` to avoid a second literal syntax in the source
/// language.
pub fn char_range(lo: &str, hi: &str) -> Option<String> {
    let (l, h) = (single_char(lo)?, single_char(hi)?);
    Some(format!("{}..{}", quote_char(l), quote_char(h)))
}

fn single_char(s: &str) -> Option<char> {
    let mut it = s.chars();
    match (it.next(), it.next()) {
        (Some(c), None) => Some(c),
        _ => None,
    }
}

fn quote_char(c: char) -> String {
    match c {
        '\'' => "'\\''".to_string(),
        '\\' => "'\\\\'".to_string(),
        '\n' => "'\\n'".to_string(),
        '\r' => "'\\r'".to_string(),
        '\t' => "'\\t'".to_string(),
        '\0' => "'\\0'".to_string(),
        c => format!("'{c}'"),
    }
}

/// Wraps in parentheses unless the fragment is already a single term.
pub fn group(fragment: &str) -> String {
    if is_atomic_fragment(fragment) {
        fragment.to_string()
    } else {
        format!("({fragment})")
    }
}

fn is_atomic_fragment(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }
    // Already parenthesised as a whole.
    if s.starts_with('(') && s.ends_with(')') && balanced(s) {
        return true;
    }
    // A bare identifier, or a literal with no top-level operators.
    !s.contains(' ')
}

fn balanced(s: &str) -> bool {
    let mut depth = 0i32;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                // Closing the initial paren before the end means the outer
                // parens do not wrap the whole fragment: `(a) ~ (b)`.
                if depth == 0 && i + 1 != s.len() {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_escape() {
        assert_eq!(string("let", false), "\"let\"");
        assert_eq!(string("AND", true), "^\"AND\"");
        // Folding a literal with no letters is a no-op, so no `^` is emitted.
        assert_eq!(string("=", true), "\"=\"");
        assert_eq!(string(" ", true), "\" \"");
        assert_eq!(string("a\"b", false), "\"a\\\"b\"");
        assert_eq!(string("\n", false), "\"\\n\"");
    }

    #[test]
    fn ranges_use_single_quotes() {
        assert_eq!(char_range("a", "z").unwrap(), "'a'..'z'");
        assert_eq!(char_range("0", "9").unwrap(), "'0'..'9'");
        assert_eq!(char_range("ab", "z"), None);
    }

    #[test]
    fn grouping_does_not_double_wrap() {
        assert_eq!(group("IDENT"), "IDENT");
        assert_eq!(group("(a | b)"), "(a | b)");
        assert_eq!(group("a ~ b"), "(a ~ b)");
        // Two separately-parenthesised terms still need an outer group.
        assert_eq!(group("(a) ~ (b)"), "((a) ~ (b))");
    }
}
