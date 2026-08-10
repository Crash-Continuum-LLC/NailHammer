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
                fn_addr: None,
            }],
            awaiting: None,
            output: Vec::new(),
            program,
        }
    }

    /// A machine that begins inside `name` rather than at the top of the code.
    ///
    /// What a host needs to run a function concurrently: the frame is sized for
    /// that function rather than for the program, and execution starts at its
    /// body. Returns `None` if nothing of that name is compiled in.
    ///
    /// Globals are still the store passed in, so several machines started this
    /// way share exactly what a language says is shared — and nothing else,
    /// because registers belong to one machine and never escape it.
    pub fn entering(program: &'a Program<X>, globals: &'a S, name: &str) -> Option<Self> {
        let f = program.fns.get(name)?;
        let mut m = Machine::new(program, globals);
        m.frames = vec![Frame {
            regs: vec![Value::Nil; f.frame.max(1)],
            ret_pc: 0,
            ret_reg: 0,
            fn_addr: Some(f.addr),
        }];
        m.pc = f.addr;
        Some(m)
    }

    /// Hands back the value the machine asked for and lets it carry on.
    ///
    /// Suspending without asking for anything is legal, and an extension is the
    /// usual reason: `Op::Await` names a register to fill, but an [`Extension`]
    /// returning [`Flow::Suspend`] is telling its driver something rather than
    /// waiting on a value. There is nowhere to put the answer then, so it is
    /// dropped — which used to panic, leaving `Flow::Suspend` a variant an
    /// extension could return and nothing could resume from.
    pub fn resume_with(&mut self, value: Value) {
        let Some(dst) = self.awaiting.take() else {
            return;
        };
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

    /// The registers of one active frame, outermost at `0`.
    ///
    /// The raw view. Prefer [`locals`](Self::locals) unless you are reading a
    /// slot whose number you already know — a register file on its own says
    /// nothing about which slots are variables and which are scratch.
    pub fn frame_regs(&self, depth: usize) -> Option<&[Value]> {
        self.frames.get(depth).map(|f| f.regs.as_slice())
    }

    /// Every **named** variable live in one frame, as `(name, value)`.
    ///
    /// This is the query a debugger wants, and it is the reason
    /// [`DebugInfo`](crate::program::DebugInfo) is keyed by address: the frame
    /// says which code it is running, that selects the slot-name table, and the
    /// table lines up with the registers by index.
    ///
    /// Empty when the compiler recorded no names, which is the default. It
    /// never fails and never guesses — an unnamed slot is a temporary, and a
    /// temporary is not a variable.
    pub fn locals(&self, depth: usize) -> impl Iterator<Item = (&str, &Value)> + '_ {
        let frame = self.frames.get(depth);
        let names: &[Option<String>] = match frame {
            None => &[],
            Some(f) => match f.fn_addr {
                Some(addr) => self
                    .program
                    .debug
                    .frames
                    .get(&addr)
                    .map_or(&[][..], Vec::as_slice),
                None => &self.program.debug.top,
            },
        };
        let regs: &[Value] = frame.map_or(&[][..], |f| f.regs.as_slice());
        names
            .iter()
            .zip(regs)
            .filter_map(|(n, v)| n.as_deref().map(|n| (n, v)))
    }

    /// Everything a suspended program needs to carry on later.
    ///
    /// A `Machine` borrows its program and its store, so a host that runs many
    /// programs cannot simply keep one alive per task without tangling
    /// lifetimes. This is the way out: a suspended machine's state is a value,
    /// so a host can hold it, schedule something else, and resume later — which
    /// is the whole reason `Step::Awaiting` exists.
    ///
    /// The registers are the part that is easy to forget. A program that
    /// suspends mid-expression has operands live in its frame, and losing them
    /// would make `await` work only as the first thing a program does.
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            pc: self.pc,
            frames: self.frames.clone(),
            awaiting: self.awaiting,
        }
    }

    /// Puts a machine back where a snapshot left off.
    pub fn restore(&mut self, s: Snapshot) {
        self.pc = s.pc;
        self.frames = s.frames;
        self.awaiting = s.awaiting;
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
            // Operands truncated to integers first, then divided — the same
            // `as i64` treatment the bitwise instructions below give them, so
            // `\` and `AND` agree about what an integer is.
            Op::IDiv { dst, a, b } => {
                if reg!(*b).as_num()? == 0.0 {
                    return Err("integer division by zero".into());
                }
                num2!(dst, a, b, |x: f64, y: f64| ((x as i64) / (y as i64)) as f64)
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

            // ---- sequences -------------------------------------------------
            Op::NewArray { dst, base, len } => {
                let start = *base as usize;
                let items: Vec<Value> =
                    self.frames[top].regs[start..start + *len].to_vec();
                reg!(*dst) = Value::array(items);
                Ok(Flow::Next)
            }
            Op::Entries { dst, src } => {
                let out = match &reg!(*src) {
                    Value::Array(a) => {
                        let g = a.read().expect("poisoned");
                        let mut flat = Vec::with_capacity(g.len() * 2);
                        for (i, v) in g.iter().enumerate() {
                            flat.push(Value::Num(i as f64));
                            flat.push(v.clone());
                        }
                        flat
                    }
                    Value::Object(o) => {
                        let g = o.read().expect("poisoned");
                        let mut flat = Vec::with_capacity(g.len() * 2);
                        for (k, v) in g.iter() {
                            flat.push(Value::str(k));
                            flat.push(v.clone());
                        }
                        flat
                    }
                    // Iterating something with no parts is an empty loop, not
                    // an error: `FOR EACH x IN nothing` running zero times is
                    // the same answer an empty array gives.
                    _ => Vec::new(),
                };
                reg!(*dst) = Value::array(out);
                Ok(Flow::Next)
            }
            Op::NewSized { dst, size } => {
                let n = reg!(*size).as_num()?;
                if n < 0.0 {
                    return Err(format!("an array cannot have {n} elements"));
                }
                reg!(*dst) = Value::array(vec![Value::Nil; n as usize]);
                Ok(Flow::Next)
            }
            Op::Resize { seq, size } => {
                let n = reg!(*size).as_num()?;
                if n < 0.0 {
                    return Err(format!("an array cannot have {n} elements"));
                }
                match &reg!(*seq) {
                    Value::Array(a) => {
                        // `resize` keeps what fits and default-fills the rest,
                        // which is exactly the specified behaviour — and it
                        // happens through the `Arc`, so every name for this
                        // array sees it.
                        a.write().expect("poisoned").resize(n as usize, Value::Nil);
                        Ok(Flow::Next)
                    }
                    other => Err(format!("cannot resize {other:?}")),
                }
            }
            Op::NewObject { dst, base, len } => {
                let start = *base as usize;
                let mut fields = std::collections::BTreeMap::new();
                for i in 0..*len {
                    let k = self.frames[top].regs[start + i * 2].to_string();
                    let v = self.frames[top].regs[start + i * 2 + 1].clone();
                    fields.insert(k, v);
                }
                reg!(*dst) = Value::object(fields);
                Ok(Flow::Next)
            }
            Op::Index { dst, seq, idx } => {
                // Dispatch on the *container*, not on the index. An object is
                // reached by key and an array by position, and one instruction
                // covers both — the same call `Add` makes when either side is a
                // string (VM-DESIGN.md §3.5).
                let out = match &reg!(*seq) {
                    Value::Object(o) => {
                        let key = reg!(*idx).to_string();
                        let g = o.read().expect("poisoned");
                        // A missing key is `Nil`, not an error: an object is a
                        // dictionary, and asking about an absent key is a
                        // question rather than a mistake.
                        g.get(&key).cloned().unwrap_or(Value::Nil)
                    }
                    other => index_of(other, reg!(*idx).as_num()?)?,
                };
                reg!(*dst) = out;
                Ok(Flow::Next)
            }
            Op::SetIndex { seq, idx, src } => {
                let v = reg!(*src).clone();
                if let Value::Object(o) = &reg!(*seq) {
                    let key = reg!(*idx).to_string();
                    o.write().expect("poisoned").insert(key, v);
                    return Ok(Flow::Next);
                }
                let i = reg!(*idx).as_num()?;
                match &reg!(*seq) {
                    Value::Array(a) => {
                        let mut g = a.write().expect("poisoned");
                        let n = index_usize(i, g.len() + 1)?;
                        if n == g.len() {
                            g.push(v);
                        } else {
                            g[n] = v;
                        }
                        Ok(Flow::Next)
                    }
                    other => Err(format!("cannot index into {other:?}")),
                }
            }
            Op::Len { dst, src } => {
                let n = reg!(*src)
                    .len()
                    .ok_or_else(|| format!("{:?} has no length", reg!(*src)))?;
                reg!(*dst) = Value::Num(n as f64);
                Ok(Flow::Next)
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
                    fn_addr: Some(f.addr),
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
                    awaiting: &mut self.awaiting,
                };
                // A panic in an extension is *this program's* failure, not the
                // process's.
                //
                // `exec` returns a `Result`, which says a language's own
                // instruction is expected to fail — but a `Result` does not
                // contain a panic, and an extension is the one place a host
                // runs code the VM knows nothing about. Unwinding from here
                // takes down whatever is driving: a thread, a task, or the
                // program that called it. Caught, it is the same
                // `Step::Failed` a division by zero produces.
                //
                // `AssertUnwindSafe` is honest rather than convenient: the
                // registers may be inconsistent afterwards, and nothing reads
                // them again, because the machine stops.
                //
                // A build with `panic = "abort"` gets no barrier — there is no
                // unwinding to catch. That is the profile's choice to make.
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| x.exec(&mut cx))) {
                    Ok(flow) => flow,
                    Err(payload) => Err(format!("extension panicked: {}", panic_text(&*payload))),
                }
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

/// Turns a numeric index into a position, or says why it cannot.
///
/// Indexing is **0-based**, mandated rather than configured (VM-DESIGN.md
/// §3.7). A fractional or negative index is an error rather than a truncation:
/// `a[1.5]` is a mistake in the program, and quietly reading `a[1]` would hide
/// it.
fn index_usize(i: f64, len: usize) -> Result<usize, String> {
    if i.fract() != 0.0 {
        return Err(format!("index {i} is not a whole number"));
    }
    if i < 0.0 {
        return Err(format!("index {i} is negative"));
    }
    let n = i as usize;
    if n >= len {
        return Err(format!("index {n} is out of range for length {len}"));
    }
    Ok(n)
}

fn index_of(v: &Value, i: f64) -> Result<Value, String> {
    match v {
        Value::Array(a) => {
            let g = a.read().expect("poisoned");
            Ok(g[index_usize(i, g.len())?].clone())
        }
        Value::Str(s) => {
            let chars: Vec<char> = s.chars().collect();
            let n = index_usize(i, chars.len())?;
            Ok(Value::str(&chars[n].to_string()))
        }
        other => Err(format!("cannot index into {other:?}")),
    }
}

/// A suspended program, as a value. See [`Machine::snapshot`].
#[derive(Clone, Debug)]
pub struct Snapshot {
    pc: usize,
    frames: Vec<Frame>,
    awaiting: Option<Reg>,
}

/// Deep enough for any reasonable program, shallow enough to fail before the
/// native stack does.
const MAX_DEPTH: usize = 512;

/// What a panic payload said, when it said anything.
fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        return (*s).to_string();
    }
    if let Some(s) = payload.downcast_ref::<String>() {
        return s.clone();
    }
    "no message".to_string()
}
