//! The machine.
//!
//! Ported from the register machine the compiler scaffold already generates
//! (`crates/nh-cli/src/templates/lib_compiler.rs`), which is why it is shaped
//! the way it is rather than freshly invented: that one works, and the point of
//! this crate is to stop copying it into every project.
//!
//! What changed in the move:
//!
//! * globals are reached **by slot through a [`SharedStore`]** rather than by
//!   name through a private `HashMap`, which is what lets two machines share
//!   them without a lock over the whole table;
//! * the instruction set is **generic over an extension** so a language can add
//!   commands without forking the machine;
//! * values are [`Value`], not `f64`, so a language can have strings.

use crate::op::{Cmp, ExtCx, Extension, Flow, Op, Reg};
use crate::program::{Frame, Program};
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
    program: &'a Program<X>,
    globals: &'a S,
    pc: usize,
    /// One entry per active call. The bottom frame is the program itself.
    frames: Vec<Frame>,
    awaiting: Option<Reg>,
    pub output: Vec<String>,
}

impl<'a, X: Extension, S: SharedStore> Machine<'a, X, S> {
    pub fn new(program: &'a Program<X>, globals: &'a S) -> Self {
        Machine {
            globals,
            pc: 0,
            frames: vec![Frame {
                regs: vec![Value::Nil; program.frame.max(1)],
                ret_pc: 0,
                ret_reg: 0,
            }],
            awaiting: None,
            output: Vec::new(),
            program,
        }
    }

    /// Hands back the value the machine asked for and lets it carry on.
    pub fn resume_with(&mut self, value: Value) {
        let dst = self.awaiting.take().expect("resume_with without Awaiting");
        let top = self.frames.len() - 1;
        self.frames[top].regs[dst as usize] = value;
    }

    pub fn reg(&self, r: Reg) -> &Value {
        &self.frames[self.frames.len() - 1].regs[r as usize]
    }

    /// How deep the call stack is. One means the program itself.
    pub fn depth(&self) -> usize {
        self.frames.len()
    }

    /// Runs until the program finishes, fails, or needs something.
    pub fn resume(&mut self) -> Step {
        while self.pc < self.program.code.len() {
            let op = self.program.code[self.pc].clone();
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
        let top = self.frames.len() - 1;

        macro_rules! reg {
            ($i:expr) => {
                self.frames[top].regs[$i as usize]
            };
        }
        macro_rules! num2 {
            ($dst:expr, $a:expr, $b:expr, $f:expr) => {{
                let a = reg!(*$a).as_num()?;
                let b = reg!(*$b).as_num()?;
                #[allow(clippy::redundant_closure_call)]
                let out = $f(a, b);
                reg!(*$dst) = Value::Num(out);
                Ok(Flow::Next)
            }};
        }

        match op {
            Op::LoadK { dst, value } => {
                reg!(*dst) = value.clone();
                Ok(Flow::Next)
            }
            Op::Move { dst, src } => {
                reg!(*dst) = reg!(*src).clone();
                Ok(Flow::Next)
            }

            Op::Add { dst, a, b } => {
                // `+` concatenates when either side is a string. This is a VM
                // decision and therefore the same in every language on it,
                // which is the point of sharing a machine (VM-DESIGN.md §3.5).
                match (&reg!(*a), &reg!(*b)) {
                    (Value::Str(_), _) | (_, Value::Str(_)) => {
                        let s = format!("{}{}", reg!(*a), reg!(*b));
                        reg!(*dst) = Value::str(&s);
                        Ok(Flow::Next)
                    }
                    _ => num2!(dst, a, b, |x, y| x + y),
                }
            }
            Op::Sub { dst, a, b } => num2!(dst, a, b, |x, y| x - y),
            Op::Mul { dst, a, b } => num2!(dst, a, b, |x, y| x * y),
            Op::Div { dst, a, b } => {
                if reg!(*b).as_num()? == 0.0 {
                    return Err("division by zero".into());
                }
                num2!(dst, a, b, |x, y| x / y)
            }
            Op::Rem { dst, a, b } => {
                if reg!(*b).as_num()? == 0.0 {
                    return Err("remainder by zero".into());
                }
                num2!(dst, a, b, |x: f64, y: f64| x % y)
            }
            Op::Pow { dst, a, b } => num2!(dst, a, b, |x: f64, y: f64| x.powf(y)),

            // Bitwise, on the integer part -- what a BASIC does, and what makes
            // `AND` and `&` one instruction rather than two.
            Op::And { dst, a, b } => num2!(dst, a, b, |x: f64, y: f64| ((x as i64) & (y as i64)) as f64),
            Op::Or { dst, a, b } => num2!(dst, a, b, |x: f64, y: f64| ((x as i64) | (y as i64)) as f64),
            Op::Xor { dst, a, b } => num2!(dst, a, b, |x: f64, y: f64| ((x as i64) ^ (y as i64)) as f64),
            Op::Shl { dst, a, b } => num2!(dst, a, b, |x: f64, y: f64| ((x as i64) << (y as i64)) as f64),
            Op::Shr { dst, a, b } => num2!(dst, a, b, |x: f64, y: f64| ((x as i64) >> (y as i64)) as f64),

            Op::Neg { dst, a } => {
                let v = reg!(*a).as_num()?;
                reg!(*dst) = Value::Num(-v);
                Ok(Flow::Next)
            }
            Op::BitNot { dst, a } => {
                let v = reg!(*a).as_num()?;
                reg!(*dst) = Value::Num(!(v as i64) as f64);
                Ok(Flow::Next)
            }
            Op::Not { dst, a } => {
                let t = reg!(*a).truthy();
                reg!(*dst) = Value::Bool(!t);
                Ok(Flow::Next)
            }
            Op::Compare { dst, cmp, a, b } => {
                let out = {
                    let a = &reg!(*a);
                    let b = &reg!(*b);
                    match cmp {
                        Cmp::Eq => a == b,
                        Cmp::Ne => a != b,
                        Cmp::Lt => a.as_num()? < b.as_num()?,
                        Cmp::Le => a.as_num()? <= b.as_num()?,
                        Cmp::Gt => a.as_num()? > b.as_num()?,
                        Cmp::Ge => a.as_num()? >= b.as_num()?,
                    }
                };
                reg!(*dst) = Value::Bool(out);
                Ok(Flow::Next)
            }

            // The only two instructions that touch shared state, and each
            // reaches exactly one slot.
            Op::LoadGlobal { dst, slot } => {
                reg!(*dst) = self.globals.load(*slot);
                Ok(Flow::Next)
            }
            Op::StoreGlobal { slot, src } => {
                let v = reg!(*src).clone();
                self.globals.store(*slot, v);
                Ok(Flow::Next)
            }

            Op::Jump(t) => Ok(Flow::Jump(*t)),
            Op::JumpIfFalse { src, target } => {
                if reg!(*src).truthy() {
                    Ok(Flow::Next)
                } else {
                    Ok(Flow::Jump(*target))
                }
            }
            Op::JumpIfTrue { src, target } => {
                if reg!(*src).truthy() {
                    Ok(Flow::Jump(*target))
                } else {
                    Ok(Flow::Next)
                }
            }
            Op::Print { src } => {
                let s = reg!(*src).to_string();
                self.output.push(s);
                Ok(Flow::Next)
            }
            Op::Halt => Ok(Flow::Halt),

            Op::Await { dst, src } => {
                self.awaiting = Some(*dst);
                Ok(Flow::Suspend(reg!(*src).clone()))
            }

            // ---- calls ----------------------------------------------------
            Op::Call { dst, base, argc, key, shown } => {
                let Some(f) = self.program.fns.get(key).copied() else {
                    return Err(format!("undefined function `{shown}`"));
                };
                if f.arity != *argc {
                    return Err(format!(
                        "`{shown}` takes {} argument(s), got {argc}",
                        f.arity
                    ));
                }
                // Guard the stack rather than letting infinite recursion take
                // the process down with it: a runaway program in one language
                // must not kill a host running several.
                if self.frames.len() >= MAX_DEPTH {
                    return Err(format!(
                        "call stack exceeded {MAX_DEPTH} frames, in `{shown}`"
                    ));
                }

                let mut regs = vec![Value::Nil; f.frame.max(*argc).max(1)];
                // Arguments are contiguous from `base`, so this is a slice
                // copy: the calling convention has no names in it.
                let src = &self.frames[top].regs[*base as usize..*base as usize + *argc];
                regs[..*argc].clone_from_slice(src);
                self.frames.push(Frame {
                    regs,
                    ret_pc: self.pc,
                    ret_reg: *dst,
                });
                Ok(Flow::Jump(f.addr))
            }
            Op::Return { src } => {
                let v = reg!(*src).clone();
                self.ret(v)
            }
            Op::ReturnUnit => self.ret(Value::Nil),

            Op::Ext(x) => {
                let mut cx = ExtCx {
                    regs: &mut self.frames[top].regs,
                    globals: self.globals,
                    output: &mut self.output,
                };
                x.exec(&mut cx)
            }
        }
    }

    fn ret(&mut self, v: Value) -> Result<Flow, String> {
        let Some(f) = self.frames.pop() else {
            return Err("return outside a call".into());
        };
        if self.frames.is_empty() {
            // Returning from the top level ends the program rather than
            // underflowing the stack.
            self.frames.push(f);
            return Ok(Flow::Halt);
        }
        let caller = self.frames.len() - 1;
        self.frames[caller].regs[f.ret_reg as usize] = v;
        Ok(Flow::Jump(f.ret_pc))
    }
}

/// Deep enough for any reasonable program, shallow enough to fail before the
/// native stack does.
const MAX_DEPTH: usize = 512;
