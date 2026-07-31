//! A BASIC-flavoured language that compiles to `nh-vm` bytecode.
//!
//! **This file is the C twin's, unchanged apart from the parser type.** The two
//! languages share every handler shape and the whole compiler, because what
//! differs between them is syntax — and syntax is the grammar's job.
//!
//! # What is *not* here
//!
//! There is no operator code in this crate. Not a `fn add`, not a match on a
//! comparison discriminant, nothing. `build.rs` says `.target("nh-vm")` and
//! `src/generated/vm_operators.rs` is written for us — because against a
//! machine that owns execution, "`add` emits `Op::Add`" is a consequence rather
//! than a decision (VM-DESIGN.md §7.2).
//!
//! What *is* here is the part only this language could have supplied: how
//! statements lower, and how variables find their slots.
//!
//! # Its twin
//!
//! `examples/vm-c` is the same language in BASIC's clothing. Its grammar
//! looks nothing like this one — word operators, line-oriented statements — and
//! it binds the **same roles**, so `AND` here and `&` there both become
//! `Op::And`. `tests/agree.rs` checks the two produce identical output.

use std::collections::HashMap;

use nh_vm::{Cmp, NoExt, Op, Reg, Slot, Value};

pub mod generated {
    include!("generated/mod.rs");
}
pub mod handlers;

#[derive(pest_derive::Parser)]
#[grammar = "lang.pest"]
pub struct BasicLangParser;

/// The compiler. `Out = Reg`: evaluating a node leaves its value in a register.
#[derive(Debug, Default)]
pub struct Interp {
    pub code: Vec<Op<NoExt>>,
    /// Next free register, and the high-water mark — the frame size.
    next: Reg,
    high: Reg,
    /// Variable name to global slot, assigned in first-seen order — which is
    /// why the two twins agree: they meet the same names in the same order.
    slots: HashMap<String, Slot>,
}

impl Interp {
    pub fn emit(&mut self, op: Op<NoExt>) -> usize {
        self.code.push(op);
        self.code.len() - 1
    }

    // ---- register allocation, in stack discipline --------------------------
    //
    // `free` only does anything for the *top* register, which keeps an
    // expression's temporaries contiguous. The generated operator methods call
    // `reuse`, so this allocator is part of the contract with generated code.

    pub fn alloc(&mut self) -> Reg {
        let r = self.next;
        self.next += 1;
        self.high = self.high.max(self.next);
        r
    }

    pub fn free(&mut self, r: Reg) {
        if self.next == r + 1 {
            self.next -= 1;
        }
    }

    /// Frees the operands, then takes a destination — so `Add` reuses the
    /// register its left operand was in instead of growing the frame.
    pub fn reuse(&mut self, operands: &[Reg]) -> Reg {
        for r in operands.iter().rev() {
            self.free(*r);
        }
        self.alloc()
    }

    /// Registers are per-statement scratch, so resetting keeps frames small.
    pub fn reset_regs(&mut self) {
        self.next = 0;
    }

    pub fn frame_size(&self) -> usize {
        self.high as usize + 1
    }

    // ---- variables ---------------------------------------------------------

    pub fn slot_of(&mut self, name: &str) -> Slot {
        if let Some(s) = self.slots.get(name) {
            return *s;
        }
        let s = self.slots.len() as Slot;
        self.slots.insert(name.to_string(), s);
        s
    }

    pub fn globals_needed(&self) -> usize {
        self.slots.len().max(1)
    }

    // ---- emitting ----------------------------------------------------------

    pub fn konst(&mut self, v: f64) -> Reg {
        let dst = self.alloc();
        self.emit(Op::LoadK { dst, value: Value::Num(v) });
        dst
    }

    pub fn read_var(&mut self, name: &str) -> Reg {
        let slot = self.slot_of(name);
        let dst = self.alloc();
        self.emit(Op::LoadGlobal { dst, slot });
        dst
    }

    pub fn here(&self) -> usize {
        self.code.len()
    }

    pub fn patch_to_here(&mut self, at: usize) {
        let here = self.here();
        match &mut self.code[at] {
            Op::Jump(t) | Op::JumpIfFalse { target: t, .. } | Op::JumpIfTrue { target: t, .. } => {
                *t = here
            }
            other => panic!("{other:?} at {at} is not a jump"),
        }
    }

    /// Finishes the program. Running off the end is `Done` too, but an explicit
    /// stop is what a real driver expects.
    pub fn finish(&mut self) {
        self.emit(Op::Halt);
    }
}

impl generated::dispatch::Semantics for Interp {
    type Out = Reg;
}

// Everything to do with operators, generated. This is the only mention of it.
use generated::dispatch::CompareOp;
use nh_runtime::Result;
include!("generated/vm_operators.rs");

// Wires every handler module to the trait. `without short_circuit` because this
// grammar declares no lazy operators -- there is no `&&` to write.
nh_handlers!(Interp);

// ---------------------------------------------------------------------------
// Driving it
// ---------------------------------------------------------------------------

/// A compiled program: the code, and how much state it needs to run.
pub struct Program {
    pub code: Vec<Op<NoExt>>,
    pub frame: usize,
    pub globals: usize,
}

/// Source in, bytecode out. No VM is involved yet — that is the point of the
/// split, and it is what lets a plugin compile without an execution engine.
pub fn compile(source: &str) -> std::result::Result<Program, String> {
    let mut sources = nh_runtime::SourceMap::new();
    let file = sources.add("<input>", source);
    let mut cx = nh_runtime::Ctx::new(sources);
    let mut host = Interp::default();

    generated::eval_source(&mut host, &mut cx, file).map_err(|ds| {
        ds.iter()
            .map(|d| d.message.clone())
            .collect::<Vec<_>>()
            .join("\n")
    })?;

    Ok(Program {
        frame: host.frame_size(),
        globals: host.globals_needed(),
        code: host.code,
    })
}

/// Runs it and gives back whatever it printed.
pub fn run(p: &Program) -> std::result::Result<Vec<String>, String> {
    let globals = nh_vm::DefaultStore::new(p.globals);
    let mut m = nh_vm::Machine::new(&p.code, &globals, p.frame);
    match m.resume() {
        nh_vm::Step::Done => Ok(m.output),
        nh_vm::Step::Failed(e) => Err(e),
        nh_vm::Step::Awaiting(_) => Err("this language has nothing to await".into()),
    }
}
