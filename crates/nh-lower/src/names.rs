//! Naming conventions for generated `.pest` rules.
//!
//! Every synthesised rule carries an `nh_` prefix so it can never collide with
//! a user's token or rule name. The one exception is `expr`, which is
//! deliberately user-facing: grammars reference it, and DESIGN.md §3 makes its
//! absence from the `.nh` file the point.

use std::collections::HashMap;

pub const PREFIX: &str = "nh_";

pub fn skip(name: &str) -> String {
    format!("{PREFIX}skip_{name}")
}

pub fn continuation(token: &str) -> String {
    format!("{PREFIX}cont_{token}")
}

pub fn reserved(token: &str) -> String {
    format!("{PREFIX}reserved_{token}")
}

/// The real body of a rule that has error recovery.
pub fn ok(rule: &str) -> String {
    format!("{PREFIX}ok_{rule}")
}

/// The error alternative of a rule that has error recovery.
pub fn error(rule: &str) -> String {
    format!("{PREFIX}error_{rule}")
}

/// A literal promoted to its own rule so it can carry an `expect` label.
///
/// Named after both the target and the literal, because the same character can
/// carry different messages in different rules.
pub fn expectation(target: &str, literal: &str) -> String {
    let target = target.replace('.', "_");
    format!("{PREFIX}expect_{target}_{}", symbolic(literal))
}

/// The rule for a labelled alternative: `<rule>_<label>`.
///
/// Predictable rather than pretty. `rule stmt = .. -> let` yields `stmt_let`,
/// which is also the handler module name at M2.
pub fn alternative(rule: &str, label: &str) -> String {
    format!("{rule}_{label}")
}

/// Turns an operator spelling into an identifier fragment.
///
/// `+=` becomes `plus_eq`, `<<` becomes `lt_lt`, `AND` becomes `and`. Names are
/// derived from the spelling only for *readability* of the generated `.pest`;
/// nothing semantic depends on them, since roles carry the meaning
/// (DESIGN.md §6.3).
pub fn symbolic(literal: &str) -> String {
    let mut out = String::new();
    for c in literal.chars() {
        let piece = match c {
            '+' => "plus",
            '-' => "minus",
            '*' => "star",
            '/' => "slash",
            '%' => "percent",
            '=' => "eq",
            '<' => "lt",
            '>' => "gt",
            '!' => "bang",
            '&' => "amp",
            '|' => "pipe",
            '^' => "caret",
            '~' => "tilde",
            '?' => "question",
            ':' => "colon",
            '.' => "dot",
            ',' => "comma",
            ';' => "semi",
            '@' => "at",
            '#' => "hash",
            '$' => "dollar",
            '\\' => "backslash",
            '(' => "lparen",
            ')' => "rparen",
            '[' => "lbrack",
            ']' => "rbrack",
            '{' => "lbrace",
            '}' => "rbrace",
            c if c.is_ascii_alphanumeric() => {
                out.push(c.to_ascii_lowercase());
                continue;
            }
            _ => "x",
        };
        if !out.is_empty() {
            out.push('_');
        }
        out.push_str(piece);
    }
    if out.is_empty() {
        out.push_str("op");
    }
    out
}

/// Allocates unique generated names, appending a numeric suffix on collision.
#[derive(Default)]
pub struct Allocator {
    taken: HashMap<String, usize>,
}

impl Allocator {
    pub fn reserve(&mut self, name: &str) {
        self.taken.entry(name.to_string()).or_insert(0);
    }

    pub fn alloc(&mut self, base: &str) -> String {
        if !self.taken.contains_key(base) {
            self.taken.insert(base.to_string(), 0);
            return base.to_string();
        }
        let mut n = *self.taken.get(base).expect("checked above");
        loop {
            n += 1;
            let candidate = format!("{base}_{n}");
            if !self.taken.contains_key(&candidate) {
                self.taken.insert(base.to_string(), n);
                self.taken.insert(candidate.clone(), 0);
                return candidate;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbolic_names_are_readable() {
        assert_eq!(symbolic("+"), "plus");
        assert_eq!(symbolic("+="), "plus_eq");
        assert_eq!(symbolic("<<"), "lt_lt");
        assert_eq!(symbolic("<="), "lt_eq");
        assert_eq!(symbolic("AND"), "and");
        assert_eq!(symbolic("|>"), "pipe_gt");
    }

    #[test]
    fn allocator_disambiguates() {
        let mut a = Allocator::default();
        assert_eq!(a.alloc("nh_op_eq"), "nh_op_eq");
        assert_eq!(a.alloc("nh_op_eq"), "nh_op_eq_1");
        assert_eq!(a.alloc("nh_op_eq"), "nh_op_eq_2");
    }
}
