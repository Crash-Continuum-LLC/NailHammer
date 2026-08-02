# 7. Adding a type

Pebble has numbers, text, booleans and null. Adding a fifth — a **point**,
written `(4, 2)` — touches more places than a statement does, and the interesting
part is which of them the compiler makes you visit and which it does not.

We will do it in seven steps, in the order the tool pushes you through them.

## 1. The grammar

A point is a new kind of atom:

```nh
rule atom
  = value:NUMBER              -> number
  | text:TEXT                 -> text
  | name:IDENT                -> name place
  | "(" x:expr "," y:expr ")" -> point
  | "(" inner:expr ")"        -> pass
  ;
```

Both coordinates are `expr`, so `(1 + 1, n * 2)` works and this handler will
never know they were expressions.

**Order matters, and it is worth understanding why it does not bite here.**
Both new alternatives start with `(`. Ordered choice takes the first that
matches *completely*: given `(1 + 2)` the point alternative consumes `(`, parses
`1 + 2`, looks for `,`, finds `)`, and **fails** — so the choice moves on and the
grouping alternative matches. PEG backtracks within a choice, so a prefix
collision is only a problem when one alternative can succeed on input the other
was meant to handle. Here they are distinguished by the comma.

```console
$ nh check pebble.nh
```

Clean. Worth doing before generating, because a shadowing mistake here is
exactly what the `shadow` lint exists to catch.

## 2. Generate

```console
$ nh build pebble.nh -o src/pebble.pest --rust src
ok: generated 9 file(s) in src  [1 new handler(s), 11 kept]
```

One new handler, eleven left alone. The stub:

```rust
/// * `x` — the value of the `expr` rule, already evaluated
/// * `y` — the value of the `expr` rule, already evaluated
pub fn run<H: Handlers>(host: &mut H, x: H::Out, y: H::Out, cx: &mut Ctx)
    -> Result<H::Out>
```

## 3. The variant

```rust
pub enum Value {
    Null,
    Num(f64),
    Text(String),
    Bool(bool),
    Point(f64, f64),      // new
}
```

Now build, **before writing anything else**, and read what comes back:

```console
$ cargo build
error: handler `atom_point` is not implemented. ...
  --> src/handlers/atom_point.rs:18:5

error[E0004]: non-exhaustive patterns: `&Value::Point(_, _)` not covered
  --> src/lib.rs:25:15
   |
25 |         match self {
   |               ^^^^ pattern `&Value::Point(_, _)` not covered
```

Two errors, and the second is the useful one: `Display` has to say what a point
looks like, and the compiler will not let you forget.

## 4. What the compiler catches — and what it does not

This is the part worth being honest about. Adding a variant produces errors
**only where you wrote an exhaustive match**. In Pebble that is exactly one
place: `Display`.

| Function | Compiler tells you? | Why |
|---|---|---|
| `Display` | **yes** | an exhaustive `match self` |
| `truthy` | no | written as `!matches!(..)`, which has a catch-all |
| `compare` | no | has a `_ => None` arm |
| `add`, `sub`, … | no | if-let chains falling through to "expected a number" |

So a new type gets you one free reminder and a list of decisions you have to go
find. That is not a flaw in the tool — a catch-all arm is a legitimate thing to
write — but it means "it compiles" is not the same as "the type is finished".

> If you want the compiler to walk you through *every* site, write your value
> operations as exhaustive matches from the start. It costs more arms today and
> buys you a checklist every time the language grows. Pebble deliberately does
> not, so this chapter can show you the difference.

## 5. Display

```rust
Value::Point(x, y) => write!(f, "({x}, {y})"),
```

That clears the `E0004`.

## 6. The handler

```rust
// handlers/atom_point.rs
pub fn run(_host: &mut Interp, x: Value, y: Value, cx: &mut Ctx) -> Result<Value> {
    match (x, y) {
        (Value::Num(a), Value::Num(b)) => Ok(Value::Point(a, b)),
        (a, b) => cx.err(format!("a point needs two numbers, got `{a}` and `{b}`")),
    }
}
```

The error case matters. `("a", 2)` parses fine — the grammar says two
expressions, not two numbers — so this is where the language decides that a
point is numeric. Grammars describe shape; handlers describe meaning.

## 7. The decisions the compiler did not force

Three, from the table above.

**Arithmetic.** Should `(3,4) + (1,2)` be `(4,6)`?

```rust
fn add(&mut self, l: Value, r: Value) -> Result<Value> {
    // ... text concatenation first ...
    if let (Value::Point(ax, ay), Value::Point(bx, by)) = (&l, &r) {
        return Ok(Value::Point(ax + bx, ay + by));
    }
    Ok(Value::Num(self.num(&l)? + self.num(&r)?))
}
```

Without this arm, `a + b` falls through to `self.num(..)` and reports "expected
a number, got `(3, 4)`" — which is a *correct* message for a language where
points do not add. Doing nothing is a decision too; the point is to make it
deliberately.

**Equality.** `Value` derives `PartialEq`, so `(3,4) == (3,4)` is already
`true` and ordering (`<`) already returns `false` for points, because `compare`
falls to its `_ => None` arm. Both are reasonable; neither was chosen.

**Truthiness.** `truthy` is `!matches!(v, Null | Bool(false) | Num(0.0))`, so a
point is always true — including `(0, 0)`. Is the origin falsy? Pebble says no,
because a point is a thing whether or not it is at zero:

```rust
// A point is a thing, even at the origin -- `(0, 0)` is somewhere.
```

A comment on a decision nothing forced you to make is worth more than a comment
on a line the compiler wrote for you.

## It works

```pebble
let a = (3, 4);
let b = (1, 2);
show a + b;
show a == (3, 4);
show a == b;
if (0, 0) { show "the origin is still a point"; }
```

```console
$ cargo run
(4, 6)
true
false
the origin is still a point
```

## What you did not have to touch

The parser, the precedence table, the evaluator, any other handler, or anything
under `generated/` — `+` already worked on points the moment `add` knew about
them, because `+` was never about numbers. It was about the `add` role.

---

Next: [Adding a block form](08-a-new-block.md).
