//! Semantic roles, and the spelling→role map.
//!
//! DESIGN.md §6.3: an operator's generated trait method is named after its
//! *role*, never its spelling. C's `&` and BASIC's `AND` both bind `bit_and`
//! and share one implementation, and renaming an operator in a grammar never
//! orphans handler code.
//!
//! Most entries need no explicit `-> role`, because the map below supplies one.
//! A spelling that is absent from the map and carries no `->` is an error, not
//! a guess.

/// Which operand positions a role leaves unevaluated (DESIGN.md §6.6).
///
/// Laziness changes the generated signature — a lazy operand arrives as a
/// `Thunk` rather than a value — so it is a property of the role, overridable
/// per entry with `lazy(..)`.
pub fn default_lazy(role: &str) -> &'static [&'static str] {
    match role {
        "and_then" | "or_else" | "coalesce" => &["rhs"],
        "ternary" => &["then", "else"],
        // `assign` is lazy in its *left* operand: it needs a place, not a value.
        "assign" | "compound_assign" => &["lhs"],
        _ => &[],
    }
}

/// The documented role vocabulary. A role outside this set is legal but
/// generates a *required* trait method with no default, so a custom operator
/// cannot be silently forgotten.
pub const KNOWN_ROLES: &[&str] = &[
    // arithmetic
    "add", "sub", "mul", "div", "rem", "pow", "neg", "pos", //
    // bitwise
    "bit_and", "bit_or", "bit_xor", "bit_not", "shl", "shr", "shift", //
    // logical
    "and_then", "or_else", "not", "coalesce", //
    // comparison
    "eq", "ne", "lt", "le", "gt", "ge", "compare", "equality", "relational", //
    // mutation
    "assign", "compound_assign", "inc", "dec", //
    // other
    "ternary", "range", "concat", "comma", "arrow",
];

pub fn is_known(role: &str) -> bool {
    KNOWN_ROLES.contains(&role)
}

/// The built-in spelling→role map.
///
/// Keyed by `(literal, is_prefix)`, because several spellings mean different
/// things in prefix and infix position: `-` is `sub` infix but `neg` prefix,
/// `&` is `bit_and` infix but an address-of in prefix position.
pub fn role_for(literal: &str, prefix: bool) -> Option<&'static str> {
    if prefix {
        return match literal {
            "-" => Some("neg"),
            "+" => Some("pos"),
            "!" => Some("not"),
            "~" => Some("bit_not"),
            "++" => Some("inc"),
            "--" => Some("dec"),
            _ => None,
        };
    }

    match literal {
        "+" => Some("add"),
        "-" => Some("sub"),
        "*" => Some("mul"),
        "/" => Some("div"),
        "%" => Some("rem"),
        "**" | "^^" => Some("pow"),
        "&" => Some("bit_and"),
        "|" => Some("bit_or"),
        "^" => Some("bit_xor"),
        "<<" => Some("shl"),
        ">>" => Some("shr"),
        "&&" => Some("and_then"),
        "||" => Some("or_else"),
        "??" => Some("coalesce"),
        "==" => Some("eq"),
        "!=" | "<>" => Some("ne"),
        "<" => Some("lt"),
        "<=" => Some("le"),
        ">" => Some("gt"),
        ">=" => Some("ge"),
        "=" => Some("assign"),
        "+=" | "-=" | "*=" | "/=" | "%=" | "<<=" | ">>=" | "&=" | "^=" | "|=" => {
            Some("compound_assign")
        }
        ".." | "..=" => Some("range"),
        "," => Some("comma"),
        "->" => Some("arrow"),
        _ => None,
    }
}

/// Word operators get roles too, so BASIC's `AND` binds the same `bit_and` that
/// C's `&` does. Matched case-insensitively, since word operators appear in
/// case-folding languages.
pub fn role_for_word(word: &str, prefix: bool) -> Option<&'static str> {
    let upper = word.to_ascii_uppercase();
    if prefix {
        return match upper.as_str() {
            "NOT" => Some("not"),
            _ => None,
        };
    }
    match upper.as_str() {
        "AND" => Some("bit_and"),
        "OR" => Some("bit_or"),
        "XOR" => Some("bit_xor"),
        "MOD" => Some("rem"),
        "ANDALSO" => Some("and_then"),
        "ORELSE" => Some("or_else"),
        "DIV" => Some("div"),
        _ => None,
    }
}
