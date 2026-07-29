//! Turning a grammar alternative's bindings into a handler's parameters.
//!
//! This is the difference between a handler that *is* its inputs and one that
//! goes and fetches them. The generated evaluator walks the owned AST and
//! evaluates sub-rules; a handler receives the results, typed and shaped by the
//! grammar.
//!
//! The shapes follow directly from the `.nh`:
//!
//! | grammar | parameter |
//! |---|---|
//! | `name:IDENT` | `&str` |
//! | `name:IDENT` on a `case-insensitive` token | `&Name` — adds `.key()` |
//! | `value:expr` | `Self::Out`, already evaluated |
//! | `lazy body:block` | `&Shared<Block>` — owned; `.eval(host, cx)?` runs it |
//! | `x:y?` | `Option<..>` |
//! | `x:y*` | `Vec<..>` or `&[..]` |
//!
//! Since M7 the tree is owned, so nothing here carries a lifetime tied to the
//! parse. A `lazy` parameter can be cloned onto the interpreter and run later,
//! which is what makes subroutines and jumps expressible (DESIGN.md §9).

use nh_lower::{Binding, Cardinality, LoweredAlternative};

pub struct Param {
    pub name: String,
    pub ty: String,
    /// How the evaluator produces the value from the AST node.
    pub extract: String,
    pub doc: String,
}

impl Param {
    /// `, name: Ty, name: Ty` — appended after `&mut self`.
    pub fn signature(params: &[Param]) -> String {
        params
            .iter()
            .map(|p| format!(", {}: {}", crate::ident(&p.name), p.ty))
            .collect()
    }

    pub fn doc_lines(params: &[Param]) -> String {
        params
            .iter()
            .map(|p| format!("    /// * `{}` — {}\n", p.name, p.doc))
            .collect()
    }
}

/// One parameter per distinct binding, in grammar order.
pub fn params(alt: &LoweredAlternative) -> Vec<Param> {
    let mut out = Vec::new();
    let mut seen = Vec::new();

    for b in &alt.bindings {
        if seen.contains(&b.name) {
            continue;
        }
        seen.push(b.name.clone());
        out.push(param(b));
    }
    out
}

pub(crate) fn param(b: &Binding) -> Param {
    let field = format!("node.{}", crate::ident(&b.name));

    // Four kinds of parameter, and the difference between them is the whole
    // reason the doc line exists: the type alone does not say whether a value
    // was evaluated for you or is waiting for you to run it.
    let kind = match (&b.token, &b.rule_ref, b.lazy) {
        (_, Some(rule), true) => Kind::Deferred {
            ty: format!("Shared<{}>", crate::type_name(rule)),
            doc: format!("the `{rule}` rule, **unevaluated** — `.eval(host, cx)?` runs it"),
        },
        (Some(t), _, _) if t.case_insensitive => Kind::Borrowed {
            ty: "Name".to_string(),
            doc: format!(
                "the `{}` token; folds case, so use `.key()` to look it up",
                t.name
            ),
        },
        (Some(t), _, _) => Kind::Text {
            doc: format!("the text of the `{}` token", t.name),
        },
        (None, Some(rule), _) => Kind::Value {
            eval: format!("eval_{}", crate::ident(rule)),
            doc: format!("the value of the `{rule}` rule, already evaluated"),
        },
        (None, None, _) => Kind::Text {
            doc: "the text this matched".to_string(),
        },
    };

    let (ty, extract, doc) = kind.shape(&field, b.cardinality);

    Param {
        name: b.name.clone(),
        ty,
        extract,
        doc,
    }
}

enum Kind {
    /// A token's text, borrowed from the AST.
    Text { doc: String },
    /// A `Name`, borrowed: both spellings survive.
    Borrowed { ty: String, doc: String },
    /// A sub-rule, evaluated before the handler runs.
    Value { eval: String, doc: String },
    /// A `lazy` sub-rule, handed over as owned data.
    Deferred { ty: String, doc: String },
}

impl Kind {
    fn shape(&self, field: &str, card: Cardinality) -> (String, String, String) {
        use Cardinality as C;

        // `borrow` is what one of them looks like by reference; `elem` is what
        // the *field* holds, which differs for text: `&str` borrows a `String`.
        let (borrow, elem, doc) = match self {
            Kind::Text { doc } => ("str".to_string(), "String".to_string(), doc.clone()),
            Kind::Borrowed { ty, doc } => (ty.clone(), ty.clone(), doc.clone()),
            Kind::Value { doc, .. } => ("Self::Out".to_string(), "Self::Out".to_string(), doc.clone()),
            Kind::Deferred { ty, doc } => (ty.clone(), ty.clone(), doc.clone()),
        };

        let doc = match card {
            C::One => doc,
            C::Optional => format!("{doc} (optional in the grammar)"),
            C::Many => format!(
                "{doc} (repeated in the grammar{})",
                match self {
                    Kind::Value { .. } =>
                        "; items that failed and were already reported are omitted",
                    _ => "",
                }
            ),
        };

        // Evaluating can fail, so those shapes need a loop and `?`. Everything
        // else is a borrow straight out of the node.
        let (ty, extract) = match (self, card) {
            (Kind::Value { eval, .. }, C::One) => {
                ("Self::Out".to_string(), format!("{eval}(host, &{field}, cx)?"))
            }
            (Kind::Value { eval, .. }, C::Optional) => (
                "Option<Self::Out>".to_string(),
                format!("match &{field} {{ Some(n) => Some({eval}(host, n, cx)?), None => None }}"),
            ),
            (Kind::Value { eval, .. }, C::Many) => (
                "Vec<Self::Out>".to_string(),
                format!(
                    "{{\n\
                    \x20               let mut v = Vec::new();\n\
                    \x20               for n in &{field} {{\n\
                    \x20                   match {eval}(host, n, cx) {{\n\
                    \x20                       Ok(x) => v.push(x),\n\
                    \x20                       Err(Error::AlreadyReported) => continue,\n\
                    \x20                       Err(e) => return Err(e),\n\
                    \x20                   }}\n\
                    \x20               }}\n\
                    \x20               v\n\
                    \x20           }}"
                ),
            ),

            (_, C::One) => (format!("&{borrow}"), format!("&{field}")),
            // `as_deref` for text, because the field holds `String` and the
            // parameter is `&str`; `as_ref` everywhere else.
            (Kind::Text { .. }, C::Optional) => {
                ("Option<&str>".to_string(), format!("{field}.as_deref()"))
            }
            (_, C::Optional) => (
                format!("Option<&{borrow}>"),
                format!("{field}.as_ref()"),
            ),
            (_, C::Many) => (format!("&[{elem}]"), format!("&{field}")),
        };

        (ty, extract, doc)
    }
}
