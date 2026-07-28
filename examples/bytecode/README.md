# bytecode — the same grammar, compiled instead of interpreted

The other examples interpret. This one emits instructions for a stack machine,
from a grammar that does not know the difference.

```console
$ cargo run -p bc-compiler -- examples/bytecode/sample.bc
```

It prints the bytecode, then runs it.

## What changes

One line.

```rust
type Out = Value;   // an interpreter: what a node evaluated to
type Out = ();      // this: nothing is returned; results live on the stack
```

Everything else follows from it. Handlers keep the same shape and the same
parameter names; they just emit where an interpreter would compute.

```rust
// examples/calc-interp — interpreter
fn add(&mut self, l: Value, r: Value) -> Result<Value> { Ok(Value::Num(l.num()? + r.num()?)) }

// here — compiler
fn add(&mut self, _: (), _: ()) -> Result<()> { self.emit(Op::Add); Ok(()) }
```

## Why that works

**Eager parameters are stack order.** Handler parameters are evaluated left to
right *before* the handler body runs. For a compiler, "evaluated" means
"emitted", so operand code lands before the operator's instruction with no
effort at all:

```
2 + 3 * 4     →     Push 2 · Push 3 · Push 4 · Mul · Add
```

Precedence is not consulted at emit time — it is already in the *order* of the
stream, put there by the operator driver. `add` is one line.

**`lazy` reads differently and works identically.**

| | interpreter | compiler |
|---|---|---|
| eager binding | already evaluated | already **emitted** |
| `lazy` binding | run it **when** I say | emit it **where** I say |

Without `lazy` on the `if` body, the body would already be in the instruction
stream before the handler could put a jump in front of it:

```rust
// src/handlers/stmt_iff.rs
pub fn run(host: &mut Interp, _cond: (), body: &Rc<Stmt>, cx: &mut Ctx) -> Result<()> {
    let jump = host.emit_jump_if_false();   // cond's code is already emitted
    body.eval(host, cx)?;                   // emits the body at this point
    host.patch_to_here(jump);               // now its length is known
    Ok(())
}
```

Note the inversion: this calls `.eval()` **once**, to emit a body that may run
many times. An interpreter calls it once per execution.

**`place` is a Store rather than a Load.** In an interpreter, marking an
alternative `place` is what keeps `x[f()] += 1` from calling `f` twice. Here it
is the difference between emitting a Store and emitting a Load — the target of
an assignment must not be *read*.

## What this example does not implement

`Values`. Look at `src/lib.rs`: there is no `impl Values for Interp`.

`Values` carries `truthy` and `is_null` — questions about a value. A compiler's
`Out` is not a value; it stands for something the target machine will compute
later, so it has nothing to inspect. Before those two methods were split out of
`Semantics`, this file had to write:

```rust
fn truthy(&self, _: &()) -> bool {
    unreachable!("truthiness is a runtime question, not a compile-time one")
}
```

A method it could never answer and must never be asked. Building this example
is what found that, and `tests/compile.rs` is what keeps it found.

## What it writes instead: `&&` and `||`

`operators::core` gives this language `&&` and `||`, and they are the two roles
a host **must** write itself — the generated trait gives them no default,
because a wrong one would be silent.

An interpreter satisfies them with one line, `nh_value_operators!();`, whose
bodies ask `Values::truthy`. A compiler has no value to ask about at build
time, so it emits the question:

```
a && b     →     <a> · Dup · JumpIfFalse end · Pop · <b> · end:
```

`Dup` is there because if `a` is falsy it *is* the result, so the test must not
consume it. `||` is the mirror image with `JumpIfTrue`.

This is the same trade as `if`, one level down: short-circuiting is a *decision*
to an interpreter and *control flow* to a compiler, and `lazy` is what lets one
signature mean both.

## Why `&&` is required rather than defaulted

A default would be silent. Measured: with one, deleting `nh_value_operators!()`
from `examples/calc-interp` compiled without a murmur and failed eight tests at
runtime — the exact failure mode this toolkit exists to eliminate. Now rustc
says `missing: or_else, and_then` and points at a doc comment telling you which
of the two lines to write.

## What legitimately differs

Non-local control flow. An interpreter unwinds with `Error::Signal`; a compiler
emits a jump and records its index for patching, which is host state rather
than a signal. Nothing in the generated code forces either choice, and no
shared mechanism would serve both — patching is not unwinding.

## Layout

| Path | |
|---|---|
| `bc.nh` | The grammar — the scaffold's, unchanged |
| `src/lib.rs` | The `Op` enum, the emitters, the trait impls, and a small VM |
| `src/handlers/*.rs` | One file per alternative; each emits |
| `src/generated/**` | Generated. Never edited |
| `tests/compile.rs` | Asserts on the instruction stream, not just the output |
