//! The instruction set, and how a language adds to it.
//!
//! # Why `Op<X>` is generic (VM-DESIGN.md §7.3)
//!
//! Rust enums are not extensible, but they are generic, and that is enough. The
//! core set is shared by every language; `Ext(X)` is where a language puts what
//! only it has.
//!
//! This is what replaces the earlier plan of a file format describing many
//! different machines. Two languages on this VM do not have to *agree* about
//! what `Add` means, because they are running the same `Add`.
//!
//! A language with no commands of its own instantiates `Op<NoExt>` and pays
//! nothing: `NoExt` is uninhabited, so the `Ext` variant cannot be constructed
//! and the match arm is unreachable.

use crate::store::Slot;
use crate::value::Value;

/// A register index. Registers are machine-local: one thread, no sharing, no
/// atomics (VM-DESIGN.md §7.4).
pub type Reg = u16;

#[derive(Clone, Debug)]
pub enum Op<X> {
    // ---- machine-local ----------------------------------------------------
    LoadK { dst: Reg, value: Value },
    Move { dst: Reg, src: Reg },

    Add { dst: Reg, a: Reg, b: Reg },
    Sub { dst: Reg, a: Reg, b: Reg },
    Mul { dst: Reg, a: Reg, b: Reg },
    Div { dst: Reg, a: Reg, b: Reg },
    /// Integer division: BASIC's `\`, Python's `//`, Pascal's `div`.
    ///
    /// Distinct from `Div` because the results differ — `7 / 2` is `3.5` and
    /// `7 \ 2` is `3` — and a language with both cannot express one of them by
    /// rounding the other after the fact. Paired with `Rem`, which was here
    /// without it.
    IDiv { dst: Reg, a: Reg, b: Reg },
    Rem { dst: Reg, a: Reg, b: Reg },
    Pow { dst: Reg, a: Reg, b: Reg },
    Neg { dst: Reg, a: Reg },
    Compare { dst: Reg, cmp: Cmp, a: Reg, b: Reg },
    Not { dst: Reg, a: Reg },
    BitNot { dst: Reg, a: Reg },

    // Bitwise, on the integer part of a number — which is what a BASIC does,
    // and what makes `AND` and `&` the same instruction rather than two.
    And { dst: Reg, a: Reg, b: Reg },
    Or { dst: Reg, a: Reg, b: Reg },
    Xor { dst: Reg, a: Reg, b: Reg },
    Shl { dst: Reg, a: Reg, b: Reg },
    Shr { dst: Reg, a: Reg, b: Reg },

    // ---- mutable shared, and the only thing that synchronises -------------
    //
    // By slot rather than by name, because an index beats a hash — and that is
    // the whole argument, narrower than it first appeared. An earlier version
    // of this comment claimed a name would mean "a lock held across the hash",
    // i.e. bank-wide contention. That conflates *a map* with *one lock over a
    // map*: a sharded map holds no such lock, and `bench_store` shows one
    // beating both per-slot locks at eight threads.
    //
    // So slots are the default, not the only option. A host whose globals are
    // dynamic, sparse, or shared by name across independently loaded languages
    // has a real reason to key differently, and `SharedStore` is where it does
    // so — the instruction stays a slot and the store decides what that means.
    LoadGlobal { dst: Reg, slot: Slot },
    StoreGlobal { slot: Slot, src: Reg },

    // ---- control ----------------------------------------------------------
    Jump(usize),
    JumpIfFalse { src: Reg, target: usize },
    JumpIfTrue { src: Reg, target: usize },
    Print { src: Reg },
    Halt,

    /// Suspend, so whoever drives the machine can wait on something.
    ///
    /// Carried over unchanged from the scaffolded compiler, where it already
    /// worked: nothing here mentions a runtime, a future or a thread, and the
    /// driver decides how to wait. It is the one part of the existing design
    /// that was already built for a VM it does not own.
    Await { dst: Reg, src: Reg },

    // ---- sequences ---------------------------------------------------------
    /// `[a, b, c]` — `len` values starting at `base`.
    NewArray { dst: Reg, base: Reg, len: usize },
    /// An array of `size` default elements, sized at **run time**.
    ///
    /// Distinct from `NewArray`, which builds one *from* registers: a literal
    /// knows its length when it is compiled, and `DIM a(n)` does not.
    NewSized { dst: Reg, size: Reg },
    /// Resizes an array **in place**, keeping the elements that still fit.
    ///
    /// In place matters: an array is a reference type, so a second binding and
    /// a `BYREF` parameter have to see the new size too. Replacing the value
    /// would only rebind one name.
    Resize { seq: Reg, size: Reg },
    /// A collection's contents, as a flat array of alternating key and value.
    ///
    /// An array yields 0-based indices as keys; an object yields its keys. One
    /// instruction for both, because iterating is one idea — and the same
    /// layout `NewObject` reads, so the two agree about what a pair is.
    ///
    /// **A copy.** Iterating a live collection while the body mutates it has no
    /// good answer; taking the entries once has an obvious one, and it is what
    /// lets a language promise that modifying during iteration is safe.
    Entries { dst: Reg, src: Reg },
    /// Builds an object from `len` **key/value pairs** in consecutive registers
    /// starting at `base` — so `2 * len` registers in all.
    ///
    /// Pairs rather than two runs because an object literal is written as
    /// pairs, and evaluating it left to right lands them in exactly that order.
    NewObject { dst: Reg, base: Reg, len: usize },
    /// `a[i]`. Out-of-range reads are an error, not a silent `Nil`.
    Index { dst: Reg, seq: Reg, idx: Reg },
    /// `a[i] = v`. Writing one past the end appends, which is how a program
    /// grows an array without a separate instruction.
    SetIndex { seq: Reg, idx: Reg, src: Reg },
    /// Length of a string or an array.
    Len { dst: Reg, src: Reg },

    // ---- calls -------------------------------------------------------------
    //
    // Arguments live in `base .. base + argc` -- contiguous, because the
    // allocator hands out registers in stack discipline, so a call finds its
    // arguments already in place with nobody arranging them. The callee's
    // parameters are slots `0..argc` of its own frame, which makes the calling
    // convention a copy with no names in it.
    /// `key` is looked up at run time; `shown` is what to say in an error.
    Call { dst: Reg, base: Reg, argc: usize, key: String, shown: String },
    Return { src: Reg },
    /// Falling off the end of a function returns nothing in particular.
    ReturnUnit,

    /// A language's own command.
    Ext(X),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// What an instruction did.
#[derive(Debug)]
pub enum Flow {
    /// Carry on with the next instruction.
    Next,
    /// Continue at this index instead.
    Jump(usize),
    /// Stop; the program is finished.
    Halt,
    /// Stop and hand this value out; the driver resumes when it has an answer.
    Suspend(Value),
}

/// What an extension instruction can reach.
///
/// Deliberately narrow. An extension gets the current frame's registers, the
/// shared store, and the output sink — not the machine, so it cannot reach into
/// the program counter or the frame stack and invent control flow the VM does
/// not know about. Anything it needs beyond this is a sign the core set is
/// missing an instruction.
pub struct ExtCx<'a> {
    pub regs: &'a mut [Value],
    pub globals: &'a dyn crate::store::SharedStore,
    pub output: &'a mut Vec<String>,
    /// Where a value handed back by the driver should land.
    ///
    /// Set through [`ExtCx::resume_into`] rather than directly, so an
    /// extension asks for one rather than being handed the machine's state.
    pub(crate) awaiting: &'a mut Option<Reg>,
}

impl ExtCx<'_> {
    pub fn reg(&self, r: Reg) -> &Value {
        &self.regs[r as usize]
    }

    pub fn set(&mut self, r: Reg, v: Value) {
        self.regs[r as usize] = v;
    }

    /// Suspend *for an answer*, and put it in `dst`.
    ///
    /// An extension that suspends is usually telling its driver something —
    /// start this thread, cancel that timer — and has nothing to receive. One
    /// that is asking a question does: reading a line from a file is a
    /// suspension whose whole point is the value that comes back.
    ///
    /// Only `Op::Await` could name a destination before this, so an extension
    /// could ask a question and had nowhere to be told the answer. Call it,
    /// then return [`Flow::Suspend`].
    pub fn resume_into(&mut self, dst: Reg) {
        *self.awaiting = Some(dst);
    }
}

/// A language's own instructions.
///
/// `Send + Sync` because a program may be run by a host that shares it between
/// threads, and `'static` because it is stored in the code.
pub trait Extension: Clone + std::fmt::Debug + Send + Sync + 'static {
    fn exec(&self, cx: &mut ExtCx<'_>) -> Result<Flow, String>;
}

/// The extension type for a language that has none.
///
/// Uninhabited, so `Op::Ext(..)` cannot be constructed and the arm that handles
/// it is dead code the optimiser removes. Extending costs nothing to those who
/// do not.
#[derive(Clone, Copy, Debug)]
pub enum NoExt {}

impl Extension for NoExt {
    fn exec(&self, _cx: &mut ExtCx<'_>) -> Result<Flow, String> {
        // Unreachable by construction: there is no value of this type.
        match *self {}
    }
}
