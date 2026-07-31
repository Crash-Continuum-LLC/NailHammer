//! The machine.
//!
//! Ported from the register machine the compiler scaffold already generates
//! (`crates/nh-cli/src/templates/lib_compiler.rs`), which is why it is shaped
//! the way it is rather than freshly invented: that one works, and the point of
//! this crate is to stop copying it into every project.
//!
//! Two things changed in the move:
//!
//! * globals are reached **by slot through a [`SharedStore`]** rather than by
//!   name through a private `HashMap`, which is what lets two machines share
//!   them without a lock over the whole table;
//! * the instruction set is **generic over an extension** so a language can add
//!   commands without forking the machine.

use crate::op::{Cmp, ExtCx, Extension, Flow, Op, Reg};
use crate::store::{DefaultStore, SharedStore};
use crate::value::Value;

/// Why a run stopped.
///
/// Unchanged in meaning from the scaffolded machine: nothing here names a
/// runtime, a future or a thread, so the same bytecode runs under a blocking
/// driver, a multi-threaded one, and a single-threaded one.
#[derive(Debug)]
pub enum Step {
    Done,
    Failed(String),
    /// Waiting on the value the program handed out. Resolve it however you
    /// like, then [`Machine::resume_with`].
    Awaiting(Value),
}

pub struct Machine<'a, X: Extension, S: SharedStore = DefaultStore> {
    code: &'a [Op<X>],
    globals: &'a S,
    pc: usize,
    regs: Vec<Value>,
    awaiting: Option<Reg>,
    pub output: Vec<String>,
}

impl<'a, X: Extension, S: SharedStore> Machine<'a, X, S> {
    pub fn new(code: &'a [Op<X>], globals: &'a S, frame: usize) -> Self {
        Machine {
            code,
            globals,
            pc: 0,
            regs: vec![Value::Nil; frame.max(1)],
            awaiting: None,
            output: Vec::new(),
        }
    }

    /// Hands back the value the machine asked for and lets it carry on.
    pub fn resume_with(&mut self, value: Value) {
        let dst = self.awaiting.take().expect("resume_with without Awaiting");
        self.regs[dst as usize] = value;
    }

    pub fn reg(&self, r: Reg) -> &Value {
        &self.regs[r as usize]
    }

    /// Runs until the program finishes, fails, or needs something.
    pub fn resume(&mut self) -> Step {
        while self.pc < self.code.len() {
            let op = self.code[self.pc].clone();
            self.pc += 1;

            let flow = match self.exec(&op) {
                Ok(f) => f,
                Err(e) => return Step::Failed(e),
            };

            match flow {
                Flow::Next => {}
                Flow::Jump(t) => self.pc = t,
                Flow::Halt => return Step::Done,
                Flow::Suspend(v) => return Step::Awaiting(v),
            }
        }
        Step::Done
    }

    fn exec(&mut self, op: &Op<X>) -> Result<Flow, String> {
        macro_rules! num2 {
            ($dst:expr, $a:expr, $b:expr, $f:expr) => {{
                let a = self.regs[*$a as usize].as_num()?;
                let b = self.regs[*$b as usize].as_num()?;
                #[allow(clippy::redundant_closure_call)]
                let out = $f(a, b);
                self.regs[*$dst as usize] = Value::Num(out);
                Ok(Flow::Next)
            }};
        }

        match op {
            Op::LoadK { dst, value } => {
                self.regs[*dst as usize] = value.clone();
                Ok(Flow::Next)
            }
            Op::Move { dst, src } => {
                self.regs[*dst as usize] = self.regs[*src as usize].clone();
                Ok(Flow::Next)
            }

            Op::Add { dst, a, b } => num2!(dst, a, b, |x, y| x + y),
            Op::Sub { dst, a, b } => num2!(dst, a, b, |x, y| x - y),
            Op::Mul { dst, a, b } => num2!(dst, a, b, |x, y| x * y),
            Op::Div { dst, a, b } => {
                let d = self.regs[*b as usize].as_num()?;
                if d == 0.0 {
                    // A VM decision, not a language one, and therefore the
                    // VM's to publish (VM-DESIGN.md §3.6).
                    return Err("division by zero".into());
                }
                num2!(dst, a, b, |x, y| x / y)
            }
            Op::Neg { dst, a } => {
                let v = self.regs[*a as usize].as_num()?;
                self.regs[*dst as usize] = Value::Num(-v);
                Ok(Flow::Next)
            }
            Op::Not { dst, a } => {
                let t = self.regs[*a as usize].truthy();
                self.regs[*dst as usize] = Value::Bool(!t);
                Ok(Flow::Next)
            }
            Op::Compare { dst, cmp, a, b } => {
                let a = &self.regs[*a as usize];
                let b = &self.regs[*b as usize];
                let out = match cmp {
                    Cmp::Eq => a == b,
                    Cmp::Ne => a != b,
                    Cmp::Lt => a.as_num()? < b.as_num()?,
                    Cmp::Le => a.as_num()? <= b.as_num()?,
                    Cmp::Gt => a.as_num()? > b.as_num()?,
                    Cmp::Ge => a.as_num()? >= b.as_num()?,
                };
                self.regs[*dst as usize] = Value::Bool(out);
                Ok(Flow::Next)
            }

            // The only two instructions that touch shared state, and each
            // reaches exactly one slot.
            Op::LoadGlobal { dst, slot } => {
                self.regs[*dst as usize] = self.globals.load(*slot);
                Ok(Flow::Next)
            }
            Op::StoreGlobal { slot, src } => {
                self.globals.store(*slot, self.regs[*src as usize].clone());
                Ok(Flow::Next)
            }

            Op::Jump(t) => Ok(Flow::Jump(*t)),
            Op::JumpIfFalse { src, target } => {
                if self.regs[*src as usize].truthy() {
                    Ok(Flow::Next)
                } else {
                    Ok(Flow::Jump(*target))
                }
            }
            Op::JumpIfTrue { src, target } => {
                if self.regs[*src as usize].truthy() {
                    Ok(Flow::Jump(*target))
                } else {
                    Ok(Flow::Next)
                }
            }
            Op::Print { src } => {
                self.output.push(self.regs[*src as usize].to_string());
                Ok(Flow::Next)
            }
            Op::Halt => Ok(Flow::Halt),

            Op::Await { dst, src } => {
                self.awaiting = Some(*dst);
                Ok(Flow::Suspend(self.regs[*src as usize].clone()))
            }

            Op::Ext(x) => {
                let mut cx = ExtCx {
                    regs: &mut self.regs,
                    globals: self.globals,
                    output: &mut self.output,
                };
                x.exec(&mut cx)
            }
        }
    }
}
