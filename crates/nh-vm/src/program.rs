//! A compiled program: code, functions, and how much room it needs.
//!
//! This lives here rather than in each language because it is what a *machine*
//! needs to run something, not what a compiler happens to have produced. Both
//! worked examples defined their own before this existed, identically, which is
//! the usual sign.

use std::collections::HashMap;

use crate::op::{Op, Reg};

/// Where a function starts and what it needs.
#[derive(Clone, Copy, Debug)]
pub struct FnDef {
    /// Index into the code where the body begins.
    pub addr: usize,
    pub arity: usize,
    /// Registers the body needs, parameters included.
    pub frame: usize,
}

#[derive(Debug)]
pub struct Program<X> {
    pub code: Vec<Op<X>>,
    /// Functions by key.
    ///
    /// Looked up **by name at run time**, not patched at compile time, so a
    /// function can be called before it is defined and can call itself. The key
    /// is whatever the language decided identity means — a case-folding BASIC
    /// folds it, a C does not — which is why the VM does not fold it here.
    pub fns: HashMap<String, FnDef>,
    /// Registers the top-level frame needs.
    pub frame: usize,
    /// Global slots the program touches.
    pub globals: usize,
}

impl<X> Default for Program<X> {
    fn default() -> Self {
        Program {
            code: Vec::new(),
            fns: HashMap::new(),
            frame: 1,
            globals: 0,
        }
    }
}

/// One call's registers, and where to go when it returns.
#[derive(Debug)]
pub(crate) struct Frame {
    pub regs: Vec<crate::value::Value>,
    pub ret_pc: usize,
    pub ret_reg: Reg,
}
