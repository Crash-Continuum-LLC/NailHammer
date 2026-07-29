//! {{Name}} — a **register-machine** compiler built with NailHammer,
//! line-oriented style.
//!
//! Three-address code: `Add r2, r0, r1` instead of `Load a · Load b · Add`.
//!
//! The whole of what makes this a register target is one line — `type Out =
//! Reg`. An interpreter's `Out` is a value; a stack compiler's is `()` because
//! nothing is returned; a register compiler's is **which register holds the
//! result**. The operator trait then reads three-address code as written:
//!
//! ```ignore
//! fn add(&mut self, lhs: Reg, rhs: Reg) -> Result<Reg>
//! ```

use std::collections::HashMap;

{{name_import}}
pub mod generated;
pub mod handlers;

#[derive(pest_derive::Parser)]
#[grammar = "{{name}}.pest"]
pub struct {{Name}}Parser;

/// A virtual register. Allocated in stack discipline, so an expression's
/// temporaries are always the top of the file and always contiguous.
pub type Reg = u16;

#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    LoadK { dst: Reg, value: f64 },
    /// Only *globals* are named at run time. A local is a slot, and reading one
    /// emits no instruction at all.
    LoadGlobal { dst: Reg, name: String },
    StoreGlobal { name: String, src: Reg },
    Add { dst: Reg, a: Reg, b: Reg },
    Sub { dst: Reg, a: Reg, b: Reg },
    Mul { dst: Reg, a: Reg, b: Reg },
    Div { dst: Reg, a: Reg, b: Reg },
    Neg { dst: Reg, a: Reg },
    Compare { dst: Reg, op: generated::dispatch::CompareOp, a: Reg, b: Reg },
    Rem { dst: Reg, a: Reg, b: Reg },
    Not { dst: Reg, a: Reg },
    Move { dst: Reg, src: Reg },
    Print { src: Reg },
    Jump(usize),
    JumpIfFalse { src: Reg, target: usize },
    JumpIfTrue { src: Reg, target: usize },
{{vm_ops}}}

{{host_types}}#[derive(Debug, Default)]
pub struct Interp {
    pub code: Vec<Op>,
    /// Where temporaries start. Everything below is a named local, live for
    /// the whole function; everything above is scratch.
    locals_end: Reg,
    /// Next free temporary, and the high-water mark — the frame size.
    next: Reg,
    high: Reg,
    /// Name -> slot, for the function being compiled. Empty at top level,
    /// where names are globals.
    scope: HashMap<String, Reg>,
    in_fn: bool,
{{host_state}}}

impl Interp {
    fn emit(&mut self, op: Op) -> usize {
        self.code.push(op);
        self.code.len() - 1
    }

    // ---- register allocation, in stack discipline --------------------------
    //
    // The whole allocator. `free` only does anything for the *top* register,
    // which is what keeps an expression's temporaries contiguous — and that is
    // what lets a call find its arguments in consecutive registers without
    // anyone arranging it.

    pub fn alloc(&mut self) -> Reg {
        let r = self.next;
        self.next += 1;
        self.high = self.high.max(self.next);
        r
    }

    /// Frees a temporary. **A local is never freed** — its slot belongs to the
    /// variable for the whole function, which is what lets `primary_var` hand
    /// one straight back with no instruction emitted.
    pub fn free(&mut self, r: Reg) {
        if r >= self.locals_end && self.next == r + 1 {
            self.next -= 1;
        }
    }

    /// Free the operands, then take a destination — so `Add` reuses the
    /// register its left operand was in instead of growing the frame.
    pub fn reuse(&mut self, operands: &[Reg]) -> Reg {
        for r in operands.iter().rev() {
            self.free(*r);
        }
        self.alloc()
    }

    pub fn next_reg(&self) -> Reg {
        self.next
    }

    // ---- the symbol table ---------------------------------------------------

    /// The slot holding `name`, if it is a local of the function being
    /// compiled. `None` means it is a global, reached by name at run time.
    pub fn lookup(&self, name: &str) -> Option<Reg> {
        self.scope.get(name).copied()
    }

    /// Reads a variable into *some* register. A local needs no instruction.
    pub fn read_var(&mut self, name: &str) -> Reg {
        match self.lookup(name) {
            Some(slot) => slot,
            None => {
                let dst = self.alloc();
                self.emit(Op::LoadGlobal { dst, name: name.to_string() });
                dst
            }
        }
    }

    /// Writes `value` to `name`, giving back the register the variable lives
    /// in. Inside a function a new name takes the next slot; at top level
    /// everything is a global.
    pub fn write_var(&mut self, name: &str, value: Reg) -> Reg {
        if !self.in_fn {
            self.emit(Op::StoreGlobal { name: name.to_string(), src: value });
            return value;
        }
        match self.lookup(name) {
            Some(slot) => {
                if slot != value {
                    self.emit(Op::Move { dst: slot, src: value });
                }
                self.free(value);
                slot
            }
            None => {
                // A new local takes the lowest temporary slot, which is where
                // the value already is: a statement starts with no temporaries
                // live, and the allocator hands out the bottom first.
                let slot = self.locals_end;
                if value != slot {
                    self.emit(Op::Move { dst: slot, src: value });
                }
                self.locals_end += 1;
                self.next = self.next.max(self.locals_end);
                self.high = self.high.max(self.next);
                self.scope.insert(name.to_string(), slot);
                slot
            }
        }
    }

    pub fn frame_size(&self) -> usize {
        self.high as usize + 1
    }

    fn bin(&mut self, a: Reg, b: Reg, f: impl Fn(Reg, Reg, Reg) -> Op) -> Reg {
        let dst = self.reuse(&[a, b]);
        self.emit(f(dst, a, b));
        dst
    }

    // ---- emitting ----------------------------------------------------------

    pub fn emit_const(&mut self, value: f64) -> Reg {
        let dst = self.alloc();
        self.emit(Op::LoadK { dst, value });
        dst
    }

    pub fn emit_print(&mut self, src: Reg) {
        self.emit(Op::Print { src });
    }

    pub fn here(&self) -> usize {
        self.code.len()
    }

    pub fn emit_jump(&mut self) -> usize {
        self.emit(Op::Jump(usize::MAX))
    }

    pub fn emit_jump_to(&mut self, target: usize) {
        self.emit(Op::Jump(target));
    }

    pub fn emit_jump_if_false(&mut self, src: Reg) -> usize {
        self.emit(Op::JumpIfFalse { src, target: usize::MAX })
    }

    pub fn emit_jump_if_true(&mut self, src: Reg) -> usize {
        self.emit(Op::JumpIfTrue { src, target: usize::MAX })
    }

    pub fn patch_to(&mut self, at: usize, target: usize) {
        match &mut self.code[at] {
            Op::Jump(t) | Op::JumpIfFalse { target: t, .. } | Op::JumpIfTrue { target: t, .. } => {
                *t = target
            }
            other => panic!("{other:?} at {at} is not a jump"),
        }
    }

    pub fn patch_to_here(&mut self, at: usize) {
        let here = self.here();
        self.patch_to(at, here);
    }

{{host_impl}}
}

impl generated::dispatch::Semantics for Interp {
    /// **The whole of what makes this a register machine.** Evaluating a node
    /// produces the register its result is in.
    type Out = Reg;
}

impl generated::dispatch::Operators for Interp {
    fn add(&mut self, a: Reg, b: Reg) -> nh_runtime::Result<Reg> {
        Ok(self.bin(a, b, |dst, a, b| Op::Add { dst, a, b }))
    }
    fn sub(&mut self, a: Reg, b: Reg) -> nh_runtime::Result<Reg> {
        Ok(self.bin(a, b, |dst, a, b| Op::Sub { dst, a, b }))
    }
    fn mul(&mut self, a: Reg, b: Reg) -> nh_runtime::Result<Reg> {
        Ok(self.bin(a, b, |dst, a, b| Op::Mul { dst, a, b }))
    }
    fn div(&mut self, a: Reg, b: Reg) -> nh_runtime::Result<Reg> {
        Ok(self.bin(a, b, |dst, a, b| Op::Div { dst, a, b }))
    }
    fn neg(&mut self, a: Reg) -> nh_runtime::Result<Reg> {
        let dst = self.reuse(&[a]);
        self.emit(Op::Neg { dst, a });
        Ok(dst)
    }
    fn rem(&mut self, a: Reg, b: Reg) -> nh_runtime::Result<Reg> {
        Ok(self.bin(a, b, |dst, a, b| Op::Rem { dst, a, b }))
    }
    fn not(&mut self, a: Reg) -> nh_runtime::Result<Reg> {
        let dst = self.reuse(&[a]);
        self.emit(Op::Not { dst, a });
        Ok(dst)
    }
    fn compare(
        &mut self,
        a: Reg,
        op: generated::dispatch::CompareOp,
        b: Reg,
    ) -> nh_runtime::Result<Reg> {
        let dst = self.reuse(&[a, b]);
        self.emit(Op::Compare { dst, op, a, b });
        Ok(dst)
    }

}

/// `&&` and `||`, compiled. The result lands in the *same* register either
/// way, which is what makes short-circuiting an expression rather than a
/// statement.
impl generated::dispatch::ShortCircuit for Interp {
    fn and_then(
        &mut self,
        lhs: Reg,
        rhs: std::rc::Rc<generated::ast::Expr>,
        cx: &mut nh_runtime::Ctx,
    ) -> nh_runtime::Result<Reg> {
        use generated::dispatch::Eval;
        let skip = self.emit_jump_if_false(lhs);
        let r = rhs.eval(self, cx)?;
        self.emit(Op::Move { dst: lhs, src: r });
        self.free(r);
        self.patch_to_here(skip);
        Ok(lhs)
    }

    fn or_else(
        &mut self,
        lhs: Reg,
        rhs: std::rc::Rc<generated::ast::Expr>,
        cx: &mut nh_runtime::Ctx,
    ) -> nh_runtime::Result<Reg> {
        use generated::dispatch::Eval;
        let skip = self.emit_jump_if_true(lhs);
        let r = rhs.eval(self, cx)?;
        self.emit(Op::Move { dst: lhs, src: r });
        self.free(r);
        self.patch_to_here(skip);
        Ok(lhs)
    }
}

crate::nh_handlers!(Interp, without short_circuit);

// ---------------------------------------------------------------------------
// The machine
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct Run {
    pub output: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug)]
struct Frame {
    regs: Vec<f64>,
    ret_pc: usize,
    ret_reg: Reg,
}

impl Interp {
    pub fn run(&self) -> Run {
        let mut run = Run::default();
        let mut globals: HashMap<String, f64> = HashMap::new();
        let mut frames = vec![Frame {
            regs: vec![0.0; self.frame_size()],
            ret_pc: usize::MAX,
            ret_reg: 0,
        }];

        let mut pc = 0usize;
        while pc < self.code.len() {
            let op = &self.code[pc];
            pc += 1;
            let top = frames.len() - 1;

            macro_rules! r {
                ($i:expr) => {
                    frames[top].regs[$i as usize]
                };
            }

            match op {
                Op::LoadK { dst, value } => r!(*dst) = *value,
                Op::Move { dst, src } => r!(*dst) = r!(*src),
                // An undeclared name is zero, as BASIC has always had it — and
                // as this scaffold's interpreter does. The two shapes have to
                // agree on a language question like this.
                Op::LoadGlobal { dst, name } => {
                    r!(*dst) = globals.get(name).copied().unwrap_or(0.0)
                }
                Op::StoreGlobal { name, src } => {
                    let v = r!(*src);
                    globals.insert(name.clone(), v);
                }
                Op::Add { dst, a, b } => r!(*dst) = r!(*a) + r!(*b),
                Op::Sub { dst, a, b } => r!(*dst) = r!(*a) - r!(*b),
                Op::Mul { dst, a, b } => r!(*dst) = r!(*a) * r!(*b),
                Op::Div { dst, a, b } => r!(*dst) = r!(*a) / r!(*b),
                Op::Neg { dst, a } => r!(*dst) = -r!(*a),
                Op::Rem { dst, a, b } => r!(*dst) = r!(*a) % r!(*b),
                // -1 is true in this style, which is what `NOT 0` gives.
                Op::Not { dst, a } => r!(*dst) = if r!(*a) == 0.0 { -1.0 } else { 0.0 },
                Op::Compare { dst, op, a, b } => {
                    use generated::dispatch::CompareOp as C;
                    let (x, y) = (r!(*a), r!(*b));
                    let yes = match op {
                        C::Lt => x < y,
                        C::LtEq => x <= y,
                        C::Gt => x > y,
                        C::GtEq => x >= y,
                        C::Eq => x == y,
                        C::LtGt => x != y,
                    };
                    r!(*dst) = if yes { -1.0 } else { 0.0 }
                }
                Op::Print { src } => run.output.push(format!("{}", r!(*src))),
                Op::Jump(t) => pc = *t,
                Op::JumpIfFalse { src, target } => {
                    if r!(*src) == 0.0 {
                        pc = *target
                    }
                }
                Op::JumpIfTrue { src, target } => {
                    if r!(*src) != 0.0 {
                        pc = *target
                    }
                }
{{vm_exec}}
            }
        }
        run
    }
}
