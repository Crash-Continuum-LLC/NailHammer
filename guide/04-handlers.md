# 4. Handlers are your language

Add the statements Pebble needs, then generate:

```nh
rule stmt
  = "let" name:IDENT "=" value:expr ";"   -> declare
  | "show" value:expr ";"                 -> show
  | value:expr ";"                        -> evaluate
  ;
```

```console
$ nh build pebble.nh -o src/pebble.pest --rust src
ok: generated 15 file(s) in src  [7 new handler(s), 0 kept]
```

Seven handler files, one per labelled alternative. Open one:

```rust
//! Handler for `stmt_declare`.
//!
//! From this alternative of `rule stmt`:
//!
//! ```text
//! "let" name:IDENT "=" value:expr ";" -> declare
//! ```
//!
//! Created once by `nh build --rust` and never overwritten. Edit freely.

/// * `name` — the text of the `IDENT` token
/// * `value` — the value of the `expr` rule, already evaluated
pub fn run<H: Handlers>(host: &mut H, name: &str, value: H::Out, cx: &mut Ctx)
    -> Result<H::Out>
{
    compile_error!("handler `stmt_declare` is not implemented. ...");
    cx.err("`stmt_declare` is not implemented yet")
}
```

**The bindings are the parameters.** `name:IDENT` became `name: &str`;
`value:expr` became `value: H::Out`, already evaluated, because the generated
evaluator walked the tree and ran it before calling you.

There is no parse tree in a handler. There is nothing to walk, nothing to index,
and no `pair.into_inner().nth(2)` to get wrong.

The two doc lines above the signature are not decoration — they are the only
place that says *which* of your parameters arrived evaluated and which did not.
Chapter 5 is entirely about that distinction.

## Writing them

The stub is generic over `H: Handlers` so it compiles before you have picked a
host. **Narrow it to your own type** as you fill it in — every worked example in
the repository does — and the signature gets shorter and the errors get better:

```rust
// handlers/stmt_declare.rs
use nh_runtime::{Ctx, Result};
use crate::{Interp, Value};

pub fn run(host: &mut Interp, name: &str, value: Value, _cx: &mut Ctx) -> Result<Value> {
    host.set(name, value.clone());
    Ok(value)
}
```

The rest of this book shows handlers in that narrowed form.

```rust
// handlers/atom_number.rs
pub fn run(_host: &mut Interp, value: &str, cx: &mut Ctx) -> Result<Value> {
    match value.parse() {
        Ok(n) => Ok(Value::Num(n)),
        Err(_) => cx.err(format!("`{value}` is not a number Pebble can hold")),
    }
}
```

```rust
// handlers/atom_text.rs
pub fn run(_host: &mut Interp, text: &str, _cx: &mut Ctx) -> Result<Value> {
    // The token includes its quotes, because the grammar matched them.
    Ok(Value::Text(text[1..text.len() - 1].to_string()))
}
```

`cx.err(..)` reports at the current node. You do not thread a span through — the
evaluator knows where it is.

## The type your language returns

`src/lib.rs` is yours. This chapter shows the parts of it that are about
*language design* and skips the parts that are about wiring — `Interp` itself,
the `nh_handlers!(Interp)` line, the parser derive, and `main.rs`. Those are in
[`examples/pebble/`](../examples/pebble/), which is this language finished, kept
compiling by CI. If you are building along and something below refers to a
method you have not written — `host.set`, `self.num` — that is where it lives.

Pebble's values:

```rust
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Num(f64),
    Text(String),
    Bool(bool),
}

impl generated::dispatch::Semantics for Interp {
    type Out = Value;
}
```

`type Out = Value` is the line that makes this an interpreter. A compiler sets
`type Out = Reg` — "which register holds the result" — and every handler
signature changes with it. Same grammar, same handler *names*, different shape.

## Operator semantics

The table said which method each operator binds to; here is what they mean:

```rust
impl generated::dispatch::Operators for Interp {
    fn add(&mut self, l: Value, r: Value) -> Result<Value> {
        // `+` concatenates when either side is text: the one place Pebble
        // makes a language decision inside an operator.
        if let (Value::Text(a), b) = (&l, &r) {
            return Ok(Value::Text(format!("{a}{b}")));
        }
        if let (a, Value::Text(b)) = (&l, &r) {
            return Ok(Value::Text(format!("{a}{b}")));
        }
        Ok(Value::Num(self.num(&l)? + self.num(&r)?))
    }

    fn div(&mut self, l: Value, r: Value) -> Result<Value> {
        let (a, b) = (self.num(&l)?, self.num(&r)?);
        if b == 0.0 {
            return Err(Error::runtime("division by zero".to_string()));
        }
        Ok(Value::Num(a / b))
    }
    // sub, mul, rem, neg, not, compare ...
}
```

Six comparison operators arrive at **one** method with a discriminant:

```rust
fn compare(&mut self, l: Value, op: CompareOp, r: Value) -> Result<Value> {
    use generated::dispatch::CompareOp::*;
    // ... one ordering, six answers
}
```

`CompareOp` is generated from your table. Add `<=>` to the comparison tier and a
new variant appears, and this match stops compiling until you say what it means.

## Truthiness

```rust
impl generated::dispatch::Values for Interp {
    fn truthy(&self, v: &Value) -> bool {
        !matches!(v, Value::Null | Value::Bool(false) | Value::Num(0.0))
    }
}
```

That is the entire cost of short-circuiting. `&&`, `||` and `??` ship with
correct default implementations written in terms of this one function, because
*when* to skip the right operand is universal and *what counts as false* is not.

## Run it

```console
$ cargo run
30
hello, pebble
```

from

```pebble
let width = 4;
let height = 7;
show width * height + 2;      # 30, not 44 — precedence you did not write

let name = "pebble";
show "hello, " + name;
```

---

Next: [Control flow, and what `lazy` is for](05-control-flow.md).
