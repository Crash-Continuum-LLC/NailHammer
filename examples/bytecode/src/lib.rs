//! Bc — a **bytecode compiler** built with NailHammer.
//!
//! The other examples in this repository interpret. This one emits instructions
//! for a stack machine, from a grammar (`bc.nh`) that does not know the
//! difference — it is the scaffold grammar `nh init` writes, unchanged.
//!
//! The structural difference from an interpreter host is one line: `type Out`
//! is `()` rather than a value, because on a stack machine every result is
//! communicated through the stack rather than returned. Read `README.md` for
//! what follows from that, then `src/handlers/stmt_iff.rs` for the one handler
//! that could not be written without `lazy`.
//!
//! What is generated (by `nh build bc.nh -o src/bc.pest --rust src`):
//!   * `src/bc.pest`      — the parser grammar
//!   * `src/generated/**` — the AST and its builder, the trait stack,
//!     evaluation, diagnostics
//!
//! What is yours: this file, `src/main.rs`, and `src/handlers/*.rs`.

use std::collections::HashMap;

pub mod generated;
pub mod handlers;

#[derive(pest_derive::Parser)]
#[grammar = "bc.pest"]
pub struct BcParser;

/// One instruction of a stack machine.
#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    Push(f64),
    Load(String),
    Store(String),
    Add,
    Sub,
    Mul,
    Div,
    Neg,
    Print,
    Pop,
    /// Jump if the top of the stack is zero, consuming it. The target is
    /// patched once the body's length is known.
    JumpIfFalse(usize),
}

/// The compiler.
///
/// Named `Interp` to match the other examples, so the two can be diffed.
#[derive(Debug, Default)]
pub struct Interp {
    pub code: Vec<Op>,
}

impl Interp {
    fn emit(&mut self, op: Op) {
        self.code.push(op);
    }

    pub fn emit_push(&mut self, n: f64) { self.emit(Op::Push(n)) }
    pub fn emit_load(&mut self, n: &str) { self.emit(Op::Load(n.to_string())) }
    pub fn emit_store(&mut self, n: &str) { self.emit(Op::Store(n.to_string())) }
    pub fn emit_print(&mut self) { self.emit(Op::Print) }
    pub fn emit_pop(&mut self) { self.emit(Op::Pop) }

    /// Emits a jump with an unknown target and returns its index, so the
    /// handler that knows where the body ends can fill it in.
    pub fn emit_jump_if_false(&mut self) -> usize {
        self.emit(Op::JumpIfFalse(usize::MAX));
        self.code.len() - 1
    }

    pub fn patch_to_here(&mut self, at: usize) {
        let here = self.code.len();
        if let Op::JumpIfFalse(target) = &mut self.code[at] {
            *target = here;
        }
    }

    /// Runs the compiled program. A real project would put this in its own
    /// crate; it is here to prove the bytecode is what it claims to be.
    pub fn run(&self) -> Vec<String> {
        let mut stack: Vec<f64> = Vec::new();
        let mut vars: HashMap<String, f64> = HashMap::new();
        let mut out = Vec::new();

        let mut pc = 0;
        while pc < self.code.len() {
            let op = &self.code[pc];
            pc += 1;
            match op {
                Op::Push(n) => stack.push(*n),
                Op::Load(n) => stack.push(*vars.get(n).unwrap_or(&0.0)),
                Op::Store(n) => {
                    let v = *stack.last().expect("store needs a value");
                    vars.insert(n.clone(), v);
                }
                Op::Add => { let b = stack.pop().unwrap(); let a = stack.pop().unwrap(); stack.push(a + b) }
                Op::Sub => { let b = stack.pop().unwrap(); let a = stack.pop().unwrap(); stack.push(a - b) }
                Op::Mul => { let b = stack.pop().unwrap(); let a = stack.pop().unwrap(); stack.push(a * b) }
                Op::Div => { let b = stack.pop().unwrap(); let a = stack.pop().unwrap(); stack.push(a / b) }
                Op::Neg => { let a = stack.pop().unwrap(); stack.push(-a) }
                Op::Print => out.push(format!("{}", stack.pop().unwrap())),
                Op::Pop => { stack.pop(); }
                Op::JumpIfFalse(t) => {
                    if stack.pop().unwrap() == 0.0 {
                        pc = *t;
                    }
                }
            }
        }
        out
    }
}

impl generated::dispatch::Semantics for Interp {
    // Nothing is *returned*; results live on the machine's stack.
    type Out = ();
}

// Note what is NOT here: no `impl Values for Interp`. A compiler has no values
// to inspect, so it does not claim it can. Before `Values` was split out of
// `Semantics`, this file had to write
//
//     fn truthy(&self, _: &()) -> bool { unreachable!() }
//
// — a method it could never answer and must never be asked.

impl generated::dispatch::Operators for Interp {
    // Operands were already emitted, in order, before this ran — so emitting
    // the instruction here puts it after them, which is exactly stack order.
    fn add(&mut self, _: (), _: ()) -> nh_runtime::Result<()> { self.emit(Op::Add); Ok(()) }
    fn sub(&mut self, _: (), _: ()) -> nh_runtime::Result<()> { self.emit(Op::Sub); Ok(()) }
    fn mul(&mut self, _: (), _: ()) -> nh_runtime::Result<()> { self.emit(Op::Mul); Ok(()) }
    fn div(&mut self, _: (), _: ()) -> nh_runtime::Result<()> { self.emit(Op::Div); Ok(()) }
    fn neg(&mut self, _: ()) -> nh_runtime::Result<()> { self.emit(Op::Neg); Ok(()) }

    fn assign(
        &mut self,
        place: generated::place::Place<'_, ()>,
        _value: (),
    ) -> nh_runtime::Result<()> {
        use generated::place::Place;
        match place {
            Place::PrimaryVar { name, .. } => { self.emit(Op::Store(name.to_string())); Ok(()) }
        }
    }

    fn place_read(&mut self, place: &generated::place::Place<'_, ()>) -> nh_runtime::Result<()> {
        use generated::place::Place;
        match place {
            Place::PrimaryVar { name, .. } => { self.emit(Op::Load(name.to_string())); Ok(()) }
        }
    }
}

crate::nh_handlers!(Interp);
