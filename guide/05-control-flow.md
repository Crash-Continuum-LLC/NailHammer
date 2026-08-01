# 5. Control flow, and what `lazy` is for

Everything so far arrived **already evaluated**. That is exactly wrong for a
branch: an `if` that evaluated both arms before choosing is not an `if`.

`lazy` is the marker that says "hand me this unrun".

```nh
rule stmt
  = ...
  | "if" cond:expr lazy then:block
      lazy otherwise:else_tail?             -> branch
  | "while" lazy cond:expr lazy body:block  -> loop
  ;

rule else_tail = "else" body:block -> pass;

rule block = "{" body:stmt* "}" -> block;
```

Regenerate, and look at what changed in the signatures:

```rust
// handlers/stmt_branch.rs
pub fn run(
    host: &mut Interp,
    cond: Value,                            // evaluated
    then: &Shared<Block>,                   // not evaluated
    otherwise: Option<&Shared<ElseTail>>,   // not evaluated, and may be absent
    cx: &mut Ctx,
) -> Result<Value>
```

The type says which is which. `Value` has run; `&Shared<Block>` has not.

```rust
pub fn run(..) -> Result<Value> {
    if host.is_true(&cond) {
        then.eval(host, cx)
    } else if let Some(tail) = otherwise {
        tail.eval(host, cx)
    } else {
        Ok(Value::Null)
    }
}
```

`.eval(host, cx)?` is how you run one. Call it once, twice, or not at all — that
choice is the whole content of a control-flow construct, and it is yours.

## Why `while` marks *both* operands

```nh
| "while" lazy cond:expr lazy body:block -> loop
```

The body being lazy is obvious. The condition is the one people get wrong:

```rust
pub fn run(
    host: &mut Interp,
    cond: &Shared<Expr>,
    body: &Shared<Block>,
    cx: &mut Ctx,
) -> Result<Value> {
    let mut last = Value::Null;
    loop {
        let test = cond.eval(host, cx)?;   // re-run every iteration
        if !host.is_true(&test) {
            return Ok(last);
        }
        last = body.eval(host, cx)?;
    }
}
```

An evaluated `cond` would be **one boolean, decided once** — so the loop either
never runs or never stops. The grammar is where that is decided, and the type
system carries it: an eager `cond` would arrive as `Value` and there would be
nothing to re-run.

> A bytecode compiler needs `lazy` on the condition for a different reason with
> the same shape: a loop re-tests, so the condition's *code* belongs at the top
> of the loop, which means the handler has to know where the top is before the
> condition is emitted. See `examples/vm-c`.

## `else` is its own rule

```nh
rule else_tail = "else" body:block -> pass;
```

`-> pass` again: the alternative produces exactly one node (`block`; the literal
`"else"` produces none), so it stands in for it and needs no handler. The `?` on
`otherwise:else_tail?` is what makes the parameter an `Option`.

## Cardinality is in the type

Three shapes, and the grammar picks:

| Grammar | Parameter |
|---|---|
| `value:expr` | `Value` |
| `value:expr?` | `Option<Value>` |
| `body:stmt*` | `Vec<Value>` |

Change `*` to `?` and the handler stops compiling. That is the point: a
cardinality change is a change to what the handler receives, and it should not
be possible to make it quietly.

There is one more spelling worth knowing, because it is how you write a
separated list without a helper rule:

```nh
rule args = items:expr ("," items:expr)* -> args;
```

Binding the same name twice gives **one** parameter covering every occurrence —
`items: Vec<Value>`, head included. The cardinality comes from all the
occurrences together, so the outer one being singular does not make the
accessor singular.

## Run it

```pebble
if width < height {
  show "taller than wide";
} else {
  show "wider than tall";
}

let n = 1;
let total = 0;
while n <= 5 {
  total = total + n;
  n = n + 1;
}
show total;
```

```console
$ cargo run
taller than wide
15
```

That `total = total + n` is doing something we have not covered — assignment to
a name that already exists. That is the next chapter.

---

Next: [Assignment, and `place`](06-assignment.md).
