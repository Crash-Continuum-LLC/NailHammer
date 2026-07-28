//! The built-in operator presets.
//!
//! Each is stored as **ordinary `.nh` source** and parsed with the real parser.
//! That is deliberate: DESIGN.md §6.1 says presets have no privileged status,
//! and the only way to keep that claim honest is to make them expressible in
//! the same syntax a user would write. Anything below could be pasted into a
//! grammar file verbatim, and `nh explain --source` prints exactly this text.
//!
//! Tiers are listed **lowest precedence first**, matching the `.nh` convention.

/// Returns the `.nh` source of a preset, or `None` if the name is unknown.
pub fn source(name: &str) -> Option<&'static str> {
    match name {
        "c_style" => Some(C_STYLE),
        "c_strict" => Some(C_STRICT),
        "core" => Some(CORE),
        "none" => Some(NONE),
        _ => None,
    }
}

pub const NAMES: &[&str] = &["c_style", "c_strict", "core", "none"];

/// The default preset: the full C operator set with C's bitwise/comparison
/// defect corrected.
///
/// In C, `&` binds *looser* than `==`, so `a & MASK == 0` parses as
/// `a & (MASK == 0)`. Go deliberately broke compatibility to fix this by
/// binding the bitwise operators tighter than comparison, and so do we.
///
/// It is deliberately not named `c`: a genuine C grammar ported onto this table
/// would silently change meaning, and a misleading name is worse than a longer
/// one. Use `c_strict` when you need bit-exact C.
const C_STYLE: &str = r#"
precedence {
    left    ",";
    right   "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "<<=" | ">>=" | "&=" | "^=" | "|=" -> assign;
    left    "||";
    left    "&&";
    left    "==" | "!=" | "<=" | ">=" | "<" | ">" -> compare;
    left    "|";
    left    "^";
    left    "&";
    left    "<<" | ">>" -> shift;
    left    "+" | "-";
    left    "*" | "/" | "%";
    left    "->";
    prefix  "!" | "~" | "-" | "+";
    atom    atom;
}
"#;

/// Bit-exact C precedence, defect included, for porting real C grammars.
///
/// The bitwise tier sits *looser* than equality here, which is what C actually
/// specifies. Grammars using this preset get the ambiguity lint.
const C_STRICT: &str = r#"
precedence {
    left    ",";
    right   "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "<<=" | ">>=" | "&=" | "^=" | "|=" -> assign;
    left    "||";
    left    "&&";
    left    "|";
    left    "^";
    left    "&";
    left    "==" | "!=" -> equality;
    left    "<=" | ">=" | "<" | ">" -> relational;
    left    "<<" | ">>" -> shift;
    left    "+" | "-";
    left    "*" | "/" | "%";
    left    "->";
    prefix  "!" | "~" | "-" | "+";
    atom    atom;
}
"#;

/// A modern subset: arithmetic, comparison, logical, and assignment. No
/// bitwise tier, no comma operator, no `->`.
const CORE: &str = r#"
precedence {
    right   "=" -> assign;
    left    "||";
    left    "&&";
    left    "==" | "!=" | "<=" | ">=" | "<" | ">" -> compare;
    left    "+" | "-";
    left    "*" | "/" | "%";
    prefix  "!" | "-";
    atom    atom;
}
"#;

/// The empty table — the starting point for a language with nothing in common
/// with C. See `examples/basic.nh`.
const NONE: &str = r#"
precedence {
    atom    atom;
}
"#;
