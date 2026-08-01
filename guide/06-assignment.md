# 6. Assignment, and `place`

`operators::core` already gave Pebble `=`:

```
  7  =    right   lazy(lhs)  -> assign
```

`lazy(lhs)` is the unusual part. Every other operator gets both operands
evaluated; assignment cannot, because the left side is not a value. `x = 1`
must not read `x` — it must *locate* it.

## Marking what can be assigned to

One word in the grammar:

```nh
rule atom
  = value:NUMBER        -> number
  | text:TEXT           -> text
  | name:IDENT          -> name place      // <- here
  | "(" inner:expr ")"  -> pass
  ;
```

`place` says: this alternative describes a **location**, so it may appear on the
left of an assignment. It is only legal after a label, which keeps it
distinguishable from a rule reference called `place`.

## What that generates

A `Place` enum, with one variant per marked alternative:

```rust
// src/generated/place.rs
pub enum Place<'a, Out> {
    /// From `atom_name` (`-> name place`).
    AtomName {
        /// The whole target, for diagnostics.
        span: Span,
        /// `name` — a name, not a value.
        name: &'a str,
        ...
    },
}
```

Note `name: &'a str`. Not a `Value` — the whole point is that it was not
evaluated.

Pebble marks exactly one alternative, so the enum has exactly one variant:

```rust
fn assign(&mut self, p: Place<'_, Value>, r: Value) -> Result<Value> {
    let Place::AtomName { name, .. } = p;
    self.set(name, r.clone());
    Ok(r)
}

fn place_read(&mut self, p: &Place<'_, Value>) -> Result<Value> {
    let Place::AtomName { name, .. } = p;
    match self.get(name) {
        Some(v) => Ok(v.clone()),
        None => Err(Error::runtime(format!("`{name}` is not defined"))),
    }
}
```

**Add a second assignable form and this stops compiling.** Give Pebble indexing:

```nh
  | name:IDENT "[" index:expr "]"  -> element place
```

and `Place` grows an `AtomElement` variant with `index: Out` alongside `name`,
and the two functions above become non-exhaustive matches. The compiler makes
you say what `a[i] = v` means before you can ship it.

That is the difference between this and a `postfix(lhs, op, operands)` shape,
where a new assignable form is a new positional convention nothing checks.

## `place_read` and compound assignment

`place_read` looks redundant — why not just evaluate the left side? Because
`+=` needs both halves: read the location, add, write it back. `c_style`'s
compound-assignment operators ship with defaults written in terms of `assign`
and `place_read`, so adding one is a single grammar line:

```nh
precedence override {
    right "+=" below "=" -> assign;
}
```

Nothing else. The default does the read, the add, and the write.

## Now the loop works

```pebble
let n = 1;
let total = 0;
while n <= 5 {
  total = total + n;    # assignment to an existing name
  n = n + 1;
}
show total;             # 15
```

```console
$ cargo run
30
hello, pebble
taller than wide
15
```

That is the core language: a page of grammar, a handler per alternative none of
them longer than a screen, and a `lib.rs` that is almost entirely arithmetic.

The next two chapters grow it — a new *type*, then a new kind of *block* —
because adding to a language you already have is the thing you will actually
spend your time doing.

---

Next: [Adding a type](07-a-new-type.md).
