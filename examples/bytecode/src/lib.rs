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
    /// Copies the top of the stack. Short-circuiting needs it: `a && b` has to
    /// test `a` and, if it wins, still leave `a` behind as the result.
    Dup,
    /// Jump if the top of the stack is zero, consuming it. The target is
    /// patched once the length of whatever is being skipped is known.
    JumpIfFalse(usize),
    /// Jump if the top of the stack is non-zero, consuming it.
    JumpIfTrue(usize),
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

    pub fn emit_dup(&mut self) { self.emit(Op::Dup) }

    /// Emits a jump with an unknown target and returns its index, so whoever
    /// finds out where the jump lands can fill it in.
    ///
    /// This — not a signal — is how a compiler does non-local control flow.
    /// See `README.md`.
    pub fn emit_jump_if_false(&mut self) -> usize {
        self.emit(Op::JumpIfFalse(usize::MAX));
        self.code.len() - 1
    }

    pub fn emit_jump_if_true(&mut self) -> usize {
        self.emit(Op::JumpIfTrue(usize::MAX));
        self.code.len() - 1
    }

    pub fn patch_to_here(&mut self, at: usize) {
        let here = self.code.len();
        match &mut self.code[at] {
            Op::JumpIfFalse(target) | Op::JumpIfTrue(target) => *target = here,
            other => panic!("{other:?} at {at} is not a jump"),
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
                Op::Dup => { let top = *stack.last().unwrap(); stack.push(top) }
                Op::JumpIfFalse(t) => {
                    if stack.pop().unwrap() == 0.0 {
                        pc = *t;
                    }
                }
                Op::JumpIfTrue(t) => {
                    if stack.pop().unwrap() != 0.0 {
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

    // `&&` and `||` are lazy in their right operand, so unlike every method
    // above, `rhs` arrives **unemitted** and this decides where it goes.
    //
    // These are *required* — the generated trait gives them no default, because
    // `&&` is in the grammar and a host that quietly did the wrong thing with
    // it would compile clean and misbehave at runtime. An interpreter satisfies
    // them with `nh_value_operators!();`. A compiler cannot: there is no value
    // to test at build time, so it emits the test instead.
    //
    //     a && b   ->   <a> · Dup · JumpIfFalse end · Pop · <b> · end:
    //
    // `Dup` is there because if `a` is falsy it *is* the result, so the test
    // must not consume it.
    fn and_then(
        &mut self,
        _lhs: (),
        rhs: std::rc::Rc<generated::ast::Expr>,
        cx: &mut nh_runtime::Ctx,
    ) -> nh_runtime::Result<()> {
        use generated::dispatch::Eval;
        self.emit_dup();
        let skip = self.emit_jump_if_false();
        self.emit_pop();
        rhs.eval(self, cx)?;
        self.patch_to_here(skip);
        Ok(())
    }

    /// `a || b` — the mirror image: keep `a` when it is *truthy*.
    fn or_else(
        &mut self,
        _lhs: (),
        rhs: std::rc::Rc<generated::ast::Expr>,
        cx: &mut nh_runtime::Ctx,
    ) -> nh_runtime::Result<()> {
        use generated::dispatch::Eval;
        self.emit_dup();
        let skip = self.emit_jump_if_true();
        self.emit_pop();
        rhs.eval(self, cx)?;
        self.patch_to_here(skip);
        Ok(())
    }

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
