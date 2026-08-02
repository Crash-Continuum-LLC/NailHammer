# 8. Choosing a host shape

Pebble is a tree-walking interpreter because `type Out = Value`. That one line
is most of what makes it one. Change it and the same grammar, the same handler
*names*, and the same operator table drive something else entirely.

## Three shapes

| | `type Out` | A handler does | Good for |
|---|---|---|---|
| **Interpreter** | your value type | computes and returns | REPLs, config, DSLs, learning |
| **Compiler** | `Reg` or `()` | *emits an instruction* | speed, suspension, shipping bytecode |
| **VM target** | `Reg` | emits `nh-vm` opcodes | plugging into a host that already exists |

`nh init` gives you the compiler by default and `nh init --interpreter` gives
the tree-walker. An `#[ignore]`d end-to-end test builds every style × feature ×
shape combination and asserts the two print the same thing, because a divergence
would mean something had become interpreter-shaped that should not be.

## What changes, and what does not

Compare `stmt_while` across shapes. Interpreter:

```rust
loop {
    let test = cond.eval(host, cx)?;
    if !host.is_true(&test) { return Ok(last); }
    last = body.eval(host, cx)?;
}
```

Compiler:

```rust
let top = host.here();
let test = cond.eval(host, cx)?;
let exit = host.emit_jump_if_false(test);
body.eval(host, cx)?;
host.emit_jump(top);
host.patch_to_here(exit);
```

Same grammar. Same `lazy` markers — and note that they are load-bearing for
*both*: the interpreter needs to re-run the condition, and the compiler needs to
know where the top of the loop is before emitting it.

What never changes: the grammar, the operator table, the handler filenames, and
which bindings exist.

## Targeting the shared VM

`nh-vm` is a bytecode machine that ships with NailHammer. Point a build at it:

```console
$ nh build pebble.nh -o src/pebble.pest --rust src --target nh-vm
```

and NailHammer writes `src/generated/vm_operators.rs` — the **whole**
`Operators` implementation. Against a machine that owns execution, `add` means
`Op::Add` in every language, so the body is a consequence rather than a
decision. Search a `--target nh-vm` project for `fn add` and there isn't one.

What you still write is the grammar and the statement handlers, which is where
the actual differences between languages live.

`examples/vm-c` and `examples/vm-basic` are the demonstration: two languages
with no syntax in common — `{ }` against `END IF`, `&` against `AND`, `;`
against a newline — compiling to **identical bytecode**, instruction for
instruction. A test pins it, and a second test fails if either language grows a
construct the other lacks.

Because the bytecode is shared, one host runs both. `examples/vm-host` is a
cooperative scheduler doing exactly that.

## Suspension

`nh-vm` programs can pause. `Step::Awaiting` hands control back without the VM
mentioning a runtime, a future, or a thread — and `Machine::snapshot` turns a
suspended program into a value you can store or send. That is what makes a
compiled language usable inside an async host, and it is why the scaffold
defaults to the compiler.

## Where to go next

**Read a worked example.** Each is small and makes one point:

| | |
|---|---|
| `examples/config/` | The claim that a language is many small files with no positional access |
| `examples/calc-interp/` | Operators end to end, proved by tests |
| `examples/basic-interp/` | Mini BASIC — loops, subroutines, `GOTO`, functions |
| `examples/bytecode/` | The same idea compiled: `type Out = ()`, a stack machine |
| `examples/vm-c`, `examples/vm-basic` | Two syntaxes, one bytecode |
| `examples/selfhost/` | `.nh` describing `.nh` |

**Reference material.** [USAGE.md](../USAGE.md) documents every construct in the
language. [DESIGN.md](../DESIGN.md) is the argument behind each interface,
including the parts that turned out wrong — it is kept honestly, so it is the
place to look when a decision seems strange.
[VM-DESIGN.md](../VM-DESIGN.md) covers `nh-vm` and what a language must agree
with to target a machine it did not write.

**Things Pebble does not have**, each a good exercise:

* **Functions.** `"fn" name:IDENT "(" (params:IDENT ","?)* ")" lazy body:block`
  — the parameters are bound as *names*, not values, which is why a call and a
  definition cannot share a rule.
* **Lists and indexing.** Adds a second `place` variant, which is where the
  compiler makes you say what `a[i] = v` means.
* **A custom operator.** `precedence override { left "|>" below "||" -> pipe; }`
  gives you a new trait method and nothing else to wire up.

---

That is the book. You have a language, and the parts of it you wrote are the
parts that were actually about your language.

The [appendix](15-pebble-in-full.md) has the finished grammar in one piece,
alongside every handler signature it generated — worth a look even if you built
along, because seeing the two side by side is the argument in one page.
