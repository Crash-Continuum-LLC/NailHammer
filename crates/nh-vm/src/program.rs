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

/// Names, for a debugger.
///
/// A register machine's speed comes from *not* looking names up — a local is a
/// slot index and the name is gone by the time anything runs. That is also what
/// makes a debugger impossible to add afterwards: a compiler that has discarded
/// names cannot describe them later. So the compiler records the mapping while
/// it still has it, and the machine carries it alongside the code.
///
/// Every field is empty by default. A language that wants no debugger fills in
/// nothing, allocates nothing, and pays nothing at run time.
#[derive(Clone, Debug, Default)]
pub struct DebugInfo {
    /// Slot to name for the top-level frame. Indexed by register; a slot
    /// holding a temporary is `None`, because only named locals are nameable.
    pub top: Vec<Option<String>>,
    /// Slot to name per function, keyed by the function's [`FnDef::addr`].
    ///
    /// Keyed by address rather than by name so a *live* frame can find its
    /// table: a frame records the address it jumped to, which costs a `usize`,
    /// where carrying its name would cost a string per call.
    pub frames: HashMap<usize, Vec<Option<String>>>,
    /// Global slot to name.
    pub globals: Vec<Option<String>>,
}

/// `Clone` so a host can hand the same compiled program to several machines —
/// an `Arc<Program>` shared across threads is what running work concurrently
/// looks like from outside the VM.
#[derive(Clone, Debug)]
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
    /// Names for a debugger. Empty unless the compiler filled it in.
    pub debug: DebugInfo,
}

impl<X> Default for Program<X> {
    fn default() -> Self {
        Program {
            code: Vec::new(),
            fns: HashMap::new(),
            frame: 1,
            globals: 0,
            debug: DebugInfo::default(),
        }
    }
}

/// One call's registers, and where to go when it returns.
#[derive(Clone, Debug)]
pub(crate) struct Frame {
    pub regs: Vec<crate::value::Value>,
    pub ret_pc: usize,
    pub ret_reg: Reg,
    /// The address this frame is running, or `None` for the top level.
    ///
    /// Carried so a debugger can match a live frame to its slot-name table in
    /// [`DebugInfo::frames`]. One `usize`, filled from the `FnDef` the call
    /// already looked up — no name, no allocation, nothing on the hot path.
    pub fn_addr: Option<usize>,
}
