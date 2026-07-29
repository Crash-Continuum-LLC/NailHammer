//! The shared pointer the generated AST is built from.
//!
//! # Why this is a type alias and not `Rc`
//!
//! Every rule-typed field in the owned AST is a shared pointer — that is what
//! makes the recursion finite and lets a `lazy` binding be stored. Which pointer
//! decides something the toolkit has no business deciding for you:
//!
//! ```text
//! Rc    a program tree is cheap, and stays on one thread
//! Arc   a program tree is Send + Sync, and every clone is atomic
//! ```
//!
//! Neither is right in general. A single-threaded interpreter should not pay for
//! atomics it never needs. A compiler that parses on one thread and emits on
//! another, or a VM that shares a stored function body between workers, cannot
//! use `Rc` at all — `Rc<T>` is not `Send`, so the tree simply cannot cross the
//! boundary.
//!
//! # Flipping it costs nothing
//!
//! Because the generated code and your handlers both say `Shared<T>`, the switch
//! is a cargo feature and **no signature changes**:
//!
//! ```toml
//! nh-runtime = { path = "vendor/nh-runtime", features = ["threadsafe"] }
//! ```
//!
//! That was the point of naming it. Spelling `Rc` throughout would have meant
//! rewriting every handler that takes a `lazy` binding to say `Arc` instead —
//! a change with no meaning, in files you own, for a decision made elsewhere.
//!
//! # What it does not do
//!
//! Make your host thread-safe. `Shared` decides whether the *program* can move
//! or be shared; whether your interpreter can is about your own state, and is
//! yours to decide. Nothing here assumes a runtime, an executor, or a thread
//! count.

/// `Rc` by default; `Arc` with the `threadsafe` feature.
#[cfg(not(feature = "threadsafe"))]
pub type Shared<T> = std::rc::Rc<T>;

/// `Arc`, so a program tree is `Send + Sync`.
#[cfg(feature = "threadsafe")]
pub type Shared<T> = std::sync::Arc<T>;

#[cfg(test)]
mod tests {
    use super::Shared;

    /// Whichever it is, the generated code only ever needs these two.
    #[test]
    fn it_constructs_and_derefs() {
        let s: Shared<String> = Shared::new("x".into());
        assert_eq!(&*s, "x");
        assert_eq!(Shared::strong_count(&s), 1);
    }

    /// The whole reason the feature exists. Off, this asserts nothing useful;
    /// on, it is the property somebody turned it on to get.
    #[test]
    #[cfg(feature = "threadsafe")]
    fn a_shared_tree_can_cross_a_thread() {
        fn assert_send<T: Send + Sync>() {}
        assert_send::<Shared<String>>();
    }
}
