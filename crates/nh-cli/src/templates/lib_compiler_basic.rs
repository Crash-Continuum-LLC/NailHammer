//! {{Name}} — a language built with NailHammer, **compiled** rather than
//! interpreted.
//!
//! Read `src/handlers/` first: one small file per grammar alternative, each
//! reading its inputs by name. There is no `into_inner()` anywhere and no child
//! is addressed by position.
//!
//! ## What makes this a compiler
//!
//! One line: `type Out = ()`. An interpreter's `Out` is a value a handler
//! returns; here nothing is returned, because on a stack machine every result
//! is communicated through the stack. Handlers *emit* where an interpreter
//! would *compute*.
//!
//! Two things follow, and both are already done for you below:
//!
//!   * **No `impl Values`.** `truthy` is a question about a value, and this
//!     host has none at build time — its `Out` stands for something the target
//!     machine will compute later.
//!   * **Its own `ShortCircuit`.** `nh_handlers!` writes that impl from
//!     `Values::truthy` for an interpreter. With no `truthy` to build on, this
//!     says `without short_circuit` and emits a jump instead.
//!
//! Everything else — precedence, associativity, the owned tree, error recovery,
//! `eval_source` — is identical to the interpreter scaffold. That is the point.
//!
//! What is generated (from `{{name}}.nh`, by `nh build --rust src`):
//!   * `src/{{name}}.pest`  — the parser grammar
//!   * `src/generated/**`   — the AST and its builder, the trait stack,
//!     evaluation, diagnostics, `eval_source`
//!
//! What is yours: this file, `src/main.rs`, and `src/handlers/*.rs`.

pub mod generated;
pub mod handlers;

#[derive(pest_derive::Parser)]
#[grammar = "{{name}}.pest"]
pub struct {{Name}}Parser;

/// One instruction of a stack machine.
///
/// Replace this with your own target. It is deliberately tiny: the interesting
/// claim is that the *handlers* barely change, not that this VM is good.
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
    /// Jump unconditionally. A loop's back-edge and every `EXIT`.
    Jump(usize),
    Rem,
    Not,
    /// The whole comparison tier. The discriminant rides along, so one opcode
    /// covers `=`, `<>`, `<=`, `>=`, `<` and `>`.
    Compare(generated::dispatch::CompareOp),
{{vm_ops}}}

{{host_types}}/// The compiler.
#[derive(Debug, Default)]
pub struct Interp {
    pub code: Vec<Op>,
{{host_state}}}

impl Interp {
    fn emit(&mut self, op: Op) {
        self.code.push(op);
    }

    pub fn emit_push(&mut self, n: f64) {
        self.emit(Op::Push(n))
    }
    pub fn emit_load(&mut self, n: &str) {
        self.emit(Op::Load(n.to_string()))
    }
    pub fn emit_store(&mut self, n: &str) {
        self.emit(Op::Store(n.to_string()))
    }
    pub fn emit_print(&mut self) {
        self.emit(Op::Print)
    }
    pub fn emit_pop(&mut self) {
        self.emit(Op::Pop)
    }
    pub fn emit_dup(&mut self) {
        self.emit(Op::Dup)
    }

    /// Where the next instruction will land. A loop's back-edge target.
    pub fn here(&self) -> usize {
        self.code.len()
    }

    pub fn emit_jump(&mut self) -> usize {
        self.emit(Op::Jump(usize::MAX));
        self.code.len() - 1
    }

    /// A jump whose target is already known — a loop's back-edge.
    pub fn emit_jump_to(&mut self, target: usize) {
        self.emit(Op::Jump(target));
    }

    /// Fills in a jump emitted earlier so it lands on `target`.
    pub fn patch_to(&mut self, at: usize, target: usize) {
        match &mut self.code[at] {
            Op::JumpIfFalse(t) | Op::JumpIfTrue(t) | Op::Jump(t) => *t = target,
            other => panic!("{other:?} at {at} is not a jump"),
        }
    }
{{host_impl}}

    /// Emits a jump with an unknown target and returns its index, so whoever
    /// finds out where the jump lands can fill it in.
    ///
    /// This — not an `Error::Signal` — is how a compiler does non-local control
    /// flow. An interpreter unwinds; a compiler patches.
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
            Op::JumpIfFalse(target) | Op::JumpIfTrue(target) | Op::Jump(target) => {
                *target = here
            }
            other => panic!("{other:?} at {at} is not a jump"),
        }
    }

    /// Runs the compiled program.
    ///
    /// A real project would put this in its own crate, or emit for a machine
    /// somebody else wrote. It is here so `cargo run` shows the bytecode doing
    /// what it claims.
    pub fn run(&self) -> Vec<String> {
        let mut stack: Vec<f64> = Vec::new();
        let mut vars: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
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
                Op::Add => bin(&mut stack, |a, b| a + b),
                Op::Sub => bin(&mut stack, |a, b| a - b),
                Op::Mul => bin(&mut stack, |a, b| a * b),
                Op::Div => bin(&mut stack, |a, b| a / b),
                Op::Neg => {
                    let a = stack.pop().unwrap();
                    stack.push(-a)
                }
                Op::Print => out.push(format!("{}", stack.pop().unwrap())),
                Op::Pop => {
                    stack.pop();
                }
                Op::Dup => {
                    let top = *stack.last().unwrap();
                    stack.push(top)
                }
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
                Op::Jump(t) => pc = *t,
                Op::Rem => bin(&mut stack, |a, b| a % b),
                Op::Not => {
                    let a = stack.pop().unwrap();
                    stack.push(if a == 0.0 { -1.0 } else { 0.0 })
                }
                Op::Compare(op) => {
                    use generated::dispatch::CompareOp as C;
                    let b = stack.pop().unwrap();
                    let a = stack.pop().unwrap();
                    let yes = match op {
                        C::Eq => a == b,
                        C::LtGt => a != b,
                        C::Lt => a < b,
                        C::LtEq => a <= b,
                        C::Gt => a > b,
                        C::GtEq => a >= b,
                    };
                    // -1 is true in BASIC, which is what `NOT 0` gives.
                    stack.push(if yes { -1.0 } else { 0.0 })
                }
{{vm_exec}}
            }
        }
        out
    }
}

fn bin(stack: &mut Vec<f64>, f: impl Fn(f64, f64) -> f64) {
    let b = stack.pop().unwrap();
    let a = stack.pop().unwrap();
    stack.push(f(a, b));
}

impl generated::dispatch::Semantics for Interp {
    /// Nothing is *returned*; results live on the machine's stack.
    type Out = ();
}

// Note what is NOT here: `impl Values for Interp`. `truthy` and `is_null` are
// questions about a value, and this host has none to inspect — so it does not
// claim it can answer them.

/// Operator semantics, which for a compiler means one instruction each.
///
/// Operands were already emitted, in order, before any of these ran — handler
/// parameters are evaluated left to right, and for a compiler "evaluated" means
/// "emitted". So emitting the instruction here puts it *after* its operands,
/// which is exactly stack order, and precedence ends up in the order of the
/// stream rather than in anything this file does.
impl generated::dispatch::Operators for Interp {
    fn add(&mut self, _: (), _: ()) -> nh_runtime::Result<()> {
        self.emit(Op::Add);
        Ok(())
    }
    fn sub(&mut self, _: (), _: ()) -> nh_runtime::Result<()> {
        self.emit(Op::Sub);
        Ok(())
    }
    fn mul(&mut self, _: (), _: ()) -> nh_runtime::Result<()> {
        self.emit(Op::Mul);
        Ok(())
    }
    fn div(&mut self, _: (), _: ()) -> nh_runtime::Result<()> {
        self.emit(Op::Div);
        Ok(())
    }
    fn neg(&mut self, _: ()) -> nh_runtime::Result<()> {
        self.emit(Op::Neg);
        Ok(())
    }
    fn rem(&mut self, _: (), _: ()) -> nh_runtime::Result<()> {
        self.emit(Op::Rem);
        Ok(())
    }
    fn not(&mut self, _: ()) -> nh_runtime::Result<()> {
        self.emit(Op::Not);
        Ok(())
    }

    /// One instruction for the whole comparison tier: the discriminant that
    /// picked the spelling is carried into the opcode.
    fn compare(
        &mut self,
        _lhs: (),
        op: generated::dispatch::CompareOp,
        _rhs: (),
    ) -> nh_runtime::Result<()> {
        self.emit(Op::Compare(op));
        Ok(())
    }

}

/// `&&` and `||`, compiled.
///
/// This is the one impl an interpreter never writes: `nh_handlers!` writes it
/// from `Values::truthy`. With no value to test at build time, this host says
/// `without short_circuit` below and emits the test instead of performing it:
///
/// ```text
/// a && b   ->   <a> · Dup · JumpIfFalse end · Pop · <b> · end:
/// ```
///
/// `Dup` is there because if `a` is falsy it *is* the result, so the test must
/// not consume it.
///
/// Note that `rhs` arrives **unemitted**. That is `lazy` in the grammar: an
/// interpreter reads it as "run this when I say", a compiler as "emit this
/// where I say".
impl generated::dispatch::ShortCircuit for Interp {
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

    /// The mirror image: keep `a` when it is *truthy*.
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
}

// `without short_circuit`: the impl above is mine, so do not write one.
crate::nh_handlers!(Interp, without short_circuit);
