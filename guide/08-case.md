# 8. Case, and what a name is

Some languages do not care about case. BASIC does not, SQL does not, and if you
are writing something people will type at a prompt you may not want to either.

This is a bigger change than it looks, because it splits a name in two: the
thing you **look up by**, and the thing you **print**. Getting that wrong is how
a compiler ends up reporting `counter` when the user typed `COUNTER`.

## Two independent knobs

```nh
keywords case-insensitive;                                    // 1

token IDENT = @ case-insensitive (ALPHA | "_") (ALPHA | DIGIT | "_")*;  // 2
```

They do different jobs and you can take either alone.

**`keywords case-insensitive`** folds *literals* and word operators. `show`,
`SHOW` and `Show` all match `"show"`, and a word operator like `AND` matches
`and`. Nothing about your handlers changes — a literal was never a parameter.

**`case-insensitive` on a token** folds that token, and this one *does* change
your handlers, because it changes what a name is.

## What changes

Turn both on and rebuild. Nothing complains — the grammar is fine — but the
generated trait has moved underneath you:

```rust
// before
fn stmt_declare(&mut self, name: &str,  value: Self::Out, cx: &mut Ctx) -> ...
fn stmt_define(&mut self, name: &str,  params: &[String], body: ..., cx: ...) -> ...

// after
fn stmt_declare(&mut self, name: &Name, value: Self::Out, cx: &mut Ctx) -> ...
fn stmt_define(&mut self, name: &Name, params: &[Name],   body: ..., cx: ...) -> ...
```

`&str` became `&Name`, and `&[String]` became `&[Name]`. `cargo build` tells you
every site:

```console
error[E0308]: mismatched types
   --> src/lib.rs:244:18
    |
244 |         self.set(name, r.clone());
    |              --- ^^^^ expected `&str`, found `&Name`
```

This is the good case. The change is in a *type*, so it cannot be half-done: you
will visit every place that touches a name before it compiles again.

## `Name` has two accessors, and choosing between them is the whole point

```rust
name.key()    // folded — what you look things up by
name.text()   // as typed — what you print
```

Every use is one or the other, and the rule is short: **`key()` for lookups,
`text()` for messages.**

```rust
// handlers/atom_name.rs
pub fn run(host: &mut Interp, name: &Name, cx: &mut Ctx) -> Result<Value> {
    match host.get(name.key()) {
        Some(v) => Ok(v.clone()),
        None => cx.err(format!("`{}` is not defined", name.text())),
    }
}
```

One line uses each. `key()` is why `Total` and `TOTAL` find the same variable;
`text()` is why the error says what the user actually wrote:

```console
$ pebble t.pebble
error: `NotDefined` is not defined
```

Not ``error: `notdefined` is not defined``, which reads like a bug in your
compiler.

**`key()` does not exist in a case-sensitive grammar.** Calling it is a compile
error rather than a silent no-op, so a symbol-table lookup cannot quietly forget
to fold.

## Storing a function

The same split, one level up — the map is keyed by `key()`, and the parameter
names are folded on the way in so that `RETURN n * 2` finds the parameter `N`:

```rust
host.define(
    name.key(),
    Function {
        params: params.iter().map(|p| p.key().to_string()).collect(),
        body: body.clone(),
    },
);
```

And the duplicate-parameter check has to compare folded, or `fn f(A, a)` slips
through:

```rust
if params[..i].iter().any(|q| q.key() == p.key()) {
    return cx.err(format!("parameter `{}` is bound twice", p.text()));
}
```

That is the shape of every case-insensitive decision you will make: compare and
store folded, report as typed.

## It works

```pebble
LET Width = 4;
Show WIDTH * 2;

FN Double(N) { RETURN n * 2; }
show DOUBLE(21);

let Total = 0;
WHILE total < 3 { Total = TOTAL + 1; }
show ToTaL;
```

```console
$ cargo run
8
42
3
```

Keywords in any case, identifiers in any case, and one variable no matter how it
is spelled.

## When not to

Case-insensitivity is a promise you cannot easily withdraw, and it has costs:
`getUserName` and `getusername` become the same name, so a language that folds
gives up camelCase as a distinguishing device. Fold if your users expect it —
BASIC, SQL, config formats, anything typed at a prompt. Do not fold because it
seems friendlier.

Pebble as shipped in [`examples/pebble/`](../examples/pebble/) stays
case-sensitive, so the rest of this book reads as it did. If you want a complete
case-insensitive language to read, [`examples/basic-interp/`](../examples/basic-interp/)
is built on both knobs.

---

Next: [Adding a type](09-a-new-type.md).
