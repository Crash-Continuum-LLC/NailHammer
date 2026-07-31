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
    Neg { dst: Reg, a: Reg },
    Compare { dst: Reg, cmp: Cmp, a: Reg, b: Reg },
    Not { dst: Reg, a: Reg },

    // ---- mutable shared, and the only thing that synchronises -------------
    //
    // By slot rather than by name: a name would mean a map lookup under a lock
    // held across the hash, which is the bank-wide contention this design is
    // built to avoid.
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
}

impl ExtCx<'_> {
    pub fn reg(&self, r: Reg) -> &Value {
        &self.regs[r as usize]
    }

    pub fn set(&mut self, r: Reg, v: Value) {
        self.regs[r as usize] = v;
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
