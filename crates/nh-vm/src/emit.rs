//! The contract between generated code and a compiler.
//!
//! # Why this exists
//!
//! Generated operator code calls seven methods on your type — `emit`, `alloc`,
//! `free`, `reuse`, `slot_of`, `read_var`, `patch_to_here`. Before this trait
//! that was an **implicit** contract: no list, no documentation, and a missing
//! method surfaced as `no method named 'reuse' found`, in a file the author did
//! not write, one method at a time.
//!
//! It was also the largest thing being billed to a language author. The two
//! worked examples were 158 lines each and differed by *two* — the name of the
//! parser type. Everything else, the register allocator included, was the same
//! code written twice, and would have been written again by anybody else.
//!
//! # What a host writes
//!
//! ```
//! use nh_vm::{Emit, Emitter, NoExt};
//!
//! #[derive(Default)]
//! struct Interp {
//!     emit: Emit<NoExt>,
//! }
//!
//! impl Emitter for Interp {
//!     type Ext = NoExt;
//!     fn emit_state(&mut self) -> &mut Emit<NoExt> { &mut self.emit }
//!     fn emit_state_ref(&self) -> &Emit<NoExt> { &self.emit }
//! }
//! ```
//!
//! Two methods, and the other seven arrive with it.

use std::collections::HashMap;

use crate::op::{Op, Reg};
use crate::program::{DebugInfo, Program};
use crate::store::Slot;
use crate::value::Value;

/// A compiler's working state: the code so far, its registers, and its globals.
#[derive(Debug)]
pub struct Emit<X> {
    pub code: Vec<Op<X>>,
    /// Next free register, and the high-water mark — the frame size.
    next: Reg,
    high: Reg,
    /// Variable name to global slot, in first-seen order.
    slots: HashMap<String, Slot>,
    /// Named registers of the function being compiled: its parameters, then
    /// any variable first assigned inside the body. Empty at the top level,
    /// where every name is a global.
    ///
    /// A `Vec` rather than a map: the list is short, and order is what decides
    /// which register a name landed in.
    locals: Vec<(String, Reg)>,
    /// Where scratch begins. Registers below it belong to named locals for the
    /// whole body; everything above is temporaries.
    locals_end: Reg,
    /// Whether a function body is being compiled.
    ///
    /// Needed as well as `locals`, because a function with no parameters and no
    /// variables yet still has to make its *next* assignment a local rather
    /// than a global.
    in_fn: bool,
    /// Functions defined so far.
    fns: HashMap<String, crate::program::FnDef>,
    /// Slot-to-name tables, accumulated as each function is finished.
    ///
    /// Filled from `locals` and `slots`, which this emitter already keeps in
    /// order to compile at all — so a debuggable program costs a language
    /// nothing to ask for and nothing to opt into.
    debug: DebugInfo,
}

impl<X> Default for Emit<X> {
    fn default() -> Self {
        Emit {
            code: Vec::new(),
            next: 0,
            high: 0,
            slots: HashMap::new(),
            locals: Vec::new(),
            locals_end: 0,
            in_fn: false,
            fns: HashMap::new(),
            debug: DebugInfo::default(),
        }
    }
}

/// Inverts a name-to-index list into the index-to-name table a debugger reads.
///
/// Sparse by construction: an index nothing claimed is a temporary, and stays
/// `None` rather than being invented a name.
fn name_table(pairs: impl IntoIterator<Item = (String, usize)>) -> Vec<Option<String>> {
    let mut out: Vec<Option<String>> = Vec::new();
    for (name, i) in pairs {
        if out.len() <= i {
            out.resize(i + 1, None);
        }
        out[i] = Some(name);
    }
    out
}

impl<X> Emit<X> {
    /// Registers needed by the top-level frame.
    pub fn frame_size(&self) -> usize {
        self.high as usize + 1
    }

    /// Global slots the program touches. At least one, since a store with none
    /// would index an empty table.
    pub fn globals_needed(&self) -> usize {
        self.slots.len().max(1)
    }
}

/// Everything generated code needs from a compiler.
///
/// The two required methods are accessors; the rest are defaults, so a host
/// implements this in about six lines and can still override any of them —
/// a language with a different allocation strategy replaces `alloc` and keeps
/// the rest.
pub trait Emitter {
    /// The language's own instructions. `Extension` already requires `Debug`
    /// and `Clone`, which is what lets the defaults below report and copy an
    /// instruction without every host restating those bounds.
    type Ext: crate::op::Extension;

    fn emit_state(&mut self) -> &mut Emit<Self::Ext>;
    fn emit_state_ref(&self) -> &Emit<Self::Ext>;

    fn emit(&mut self, op: Op<Self::Ext>) -> usize {
        let s = self.emit_state();
        s.code.push(op);
        s.code.len() - 1
    }

    /// Where the next instruction will go.
    fn here(&self) -> usize {
        self.emit_state_ref().code.len()
    }

    // ---- registers, in stack discipline ------------------------------------
    //
    // `free` only does anything for the *top* register, which is what keeps an
    // expression's temporaries contiguous — and that is what lets `NewArray`
    // and a call find their operands side by side without anybody arranging
    // them.

    fn alloc(&mut self) -> Reg {
        let s = self.emit_state();
        let r = s.next;
        s.next += 1;
        s.high = s.high.max(s.next);
        r
    }

    /// Releases a scratch register.
    ///
    /// **A parameter is never freed.** Its slot belongs to the variable for the
    /// whole body, and freeing it lets the next `reuse` hand it straight back
    /// out -- so `n < 2` would write the comparison result into `n`. Only the
    /// top register is released, which is what keeps an expression's
    /// temporaries contiguous.
    fn free(&mut self, r: Reg) {
        let s = self.emit_state();
        let floor = s.locals_end;
        if r >= floor && s.next == r + 1 {
            s.next -= 1;
        }
    }

    /// Frees the operands, then takes a destination — so `Add` reuses the
    /// register its left operand was in instead of growing the frame.
    fn reuse(&mut self, operands: &[Reg]) -> Reg {
        for r in operands.iter().rev() {
            self.free(*r);
        }
        self.alloc()
    }

    /// Releases a statement's scratch registers.
    ///
    /// Resets to **above the parameters**, not to zero. Inside a function the
    /// parameters occupy registers `0..arity` for the whole body, so resetting
    /// to zero hands them out again as scratch -- and the symptom is a
    /// recursive call returning the base case, because the argument was
    /// overwritten between the test and the recursion.
    fn reset_regs(&mut self) {
        let s = self.emit_state();
        s.next = s.locals_end;
    }

    fn frame_size(&self) -> usize {
        self.emit_state_ref().frame_size()
    }

    // ---- variables ---------------------------------------------------------

    /// The slot for `name`, assigning one in first-seen order.
    ///
    /// First-seen order is why two languages agree: they meet the same names in
    /// the same order, so they reach the same slots without coordinating.
    fn slot_of(&mut self, name: &str) -> Slot {
        let s = self.emit_state();
        if let Some(slot) = s.slots.get(name) {
            return *slot;
        }
        let slot = s.slots.len() as Slot;
        s.slots.insert(name.to_string(), slot);
        slot
    }

    fn globals_needed(&self) -> usize {
        self.emit_state_ref().globals_needed()
    }

    /// Where a name lives right now: a parameter register, or nothing.
    fn local_of(&self, name: &str) -> Option<Reg> {
        self.emit_state_ref()
            .locals
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, r)| *r)
    }

    /// Whether this name is already a variable.
    ///
    /// A local in the body being compiled, or a global slot already handed out.
    /// What it is *for* is `name(args)`: a language whose call and subscript
    /// look alike has to decide which one it is, and "something has already
    /// assigned to this name" is the evidence available at compile time.
    ///
    /// Note what it does not say: nothing about a name yet to be seen. A
    /// function called before it is written is not a variable either, which is
    /// why the caller should treat "no" as "assume a call" rather than as "this
    /// is undefined".
    fn knows_var(&self, name: &str) -> bool {
        let s = self.emit_state_ref();
        s.locals.iter().any(|(n, _)| n == name) || s.slots.contains_key(name)
    }

    /// Reads a variable into a register.
    ///
    /// A parameter is already in one, so reading it emits nothing at all —
    /// which is why a function body that only touches its arguments produces no
    /// loads.
    fn read_var(&mut self, name: &str) -> Reg {
        if let Some(r) = self.local_of(name) {
            return r;
        }
        let slot = self.slot_of(name);
        let dst = self.alloc();
        self.emit(Op::LoadGlobal { dst, slot });
        dst
    }

    /// Writes a variable, wherever it lives — creating a local if it is new and
    /// a function is being compiled.
    ///
    /// This is the one place that decides *where a name lives*, which is what
    /// makes it correct. An earlier version only treated **parameters** as
    /// local, so a temporary assigned inside a body became a global shared by
    /// every frame: `f(n-1) + t` returned the wrong answer while `t + f(n-1)`
    /// returned the right one, because reading `t` into a register before the
    /// recursive call happened to save it. A bug that depends on which side of
    /// a `+` you write is not one anybody should have to find.
    fn store_var(&mut self, name: &str, src: Reg) {
        if let Some(r) = self.local_of(name) {
            if r != src {
                self.emit(Op::Move { dst: r, src });
            }
            return;
        }
        if self.emit_state_ref().in_fn {
            // A new name inside a body takes the next named register, which is
            // per frame — so recursion gets its own copy.
            let dst = {
                let s = self.emit_state();
                let dst = s.locals_end;
                s.locals.push((name.to_string(), dst));
                s.locals_end += 1;
                s.next = s.next.max(s.locals_end);
                s.high = s.high.max(s.next);
                dst
            };
            if dst != src {
                self.emit(Op::Move { dst, src });
            }
            return;
        }
        let slot = self.slot_of(name);
        self.emit(Op::StoreGlobal { slot, src });
    }

    // ---- functions ----------------------------------------------------------

    /// Starts a function body: parameters take registers `0..n`, which is the
    /// calling convention (`Op::Call` copies arguments into exactly those).
    ///
    /// Returns the address the body starts at.
    fn begin_fn(&mut self, params: &[String]) -> usize {
        let s = self.emit_state();
        s.locals = params
            .iter()
            .enumerate()
            .map(|(i, n)| (n.clone(), i as Reg))
            .collect();
        s.locals_end = params.len() as Reg;
        s.in_fn = true;
        s.next = s.locals_end;
        s.high = s.high.max(s.next);
        self.here()
    }

    /// Ends a function body and records it.
    ///
    /// The trailing `ReturnUnit` is unconditional: falling off the end of a
    /// function has to return *something*, and a body that already returned
    /// never reaches it.
    fn end_fn(&mut self, name: &str, addr: usize, arity: usize) {
        self.emit(Op::ReturnUnit);
        let frame = self.frame_size();
        let s = self.emit_state();
        // Before the clear: this is the only moment the body's names and its
        // register numbers exist together. Keyed by `addr`, because that is
        // what a live frame carries.
        let table = name_table(s.locals.iter().map(|(n, r)| (n.clone(), *r as usize)));
        s.debug.frames.insert(addr, table);
        s.locals.clear();
        s.locals_end = 0;
        s.in_fn = false;
        s.next = 0;
        s.fns.insert(
            name.to_string(),
            crate::program::FnDef { addr, arity, frame },
        );
    }

    // ---- constants and jumps ------------------------------------------------

    /// A constant, in a register of its own.
    ///
    /// Named for what it does, like the rest of this trait — `const` is a Rust
    /// keyword, and the alternatives were a raw identifier (`r#const`, hostile
    /// at every call site) or a deliberate misspelling. It was `konst` for a
    /// while, which is a habit borrowed from other languages: a misspelled noun
    /// in a trait where everything else is a verb.
    fn emit_const(&mut self, value: Value) -> Reg {
        let dst = self.alloc();
        self.emit(Op::LoadK { dst, value });
        dst
    }

    /// Points a jump at wherever the next instruction lands.
    ///
    /// Panics if `at` is not a jump, because that is a bug in a handler rather
    /// than a condition a program can reach: nothing a *user* writes can make a
    /// compiler patch an `Add`.
    fn patch_to_here(&mut self, at: usize) {
        let here = self.here();
        match &mut self.emit_state().code[at] {
            Op::Jump(t) | Op::JumpIfFalse { target: t, .. } | Op::JumpIfTrue { target: t, .. } => {
                *t = here
            }
            other => panic!("instruction {at} is {other:?}, not a jump"),
        }
    }

    /// Finishes the program and hands back everything a machine needs.
    fn finish(&mut self) -> Program<Self::Ext> {
        self.emit(Op::Halt);
        let frame = self.frame_size();
        let globals = self.globals_needed();
        let s = self.emit_state();
        let mut debug = std::mem::take(&mut s.debug);
        debug.globals = name_table(s.slots.iter().map(|(n, sl)| (n.clone(), *sl as usize)));
        debug.top = name_table(s.locals.iter().map(|(n, r)| (n.clone(), *r as usize)));
        Program {
            code: std::mem::take(&mut s.code),
            fns: std::mem::take(&mut s.fns),
            frame,
            globals,
            debug,
        }
    }
}
