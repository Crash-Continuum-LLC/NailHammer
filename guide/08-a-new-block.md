# 8. Adding a block form

Pebble's blocks are `{ }`, because `if` and `while` needed somewhere to put
statements. This chapter adds a different shape — a **delimited command**,
BASIC-style:

```pebble
begin frame
  show "Pebble";
  show a;
  show 6 * 7;
end frame
```

`frame` collects everything shown inside it and prints it with a border. That
makes it a good example precisely because it is *not* control flow: the body
runs exactly once, in order, and the handler still needs it `lazy` — for a
reason worth seeing.

## 1. Reserve the words

```nh
reserved from IDENT { "let" "show" "if" "else" "while" "not"
                      "begin" "end" "frame" }
```

Three new keywords. Without this, `begin` would still parse as a keyword where
the grammar expects one — but `beginning` would lex as `begin` followed by
`ning`, and a variable called `frame` would become a syntax error somewhere
confusing. `reserved from` guards both directions at once.

If you would rather keep `frame` usable as a variable name — a *contextual*
keyword — use `guard from` instead, which adds the boundary guard without the
reservation.

## 2. The rule

```nh
rule stmt
  = ...
  | "begin" "frame" lazy body:stmt* "end" "frame" -> frame
  | value:expr ";"                                -> evaluate
  ;
```

Three things worth separating out.

**The delimiters are literals, not a block rule.** `if` and `while` reuse
`rule block = "{" body:stmt* "}" -> block;` because they share a shape. `frame`
has its own opening and closing text, so it carries its own — and gets its own
node rather than borrowing `block`'s.

**`body:stmt*` binds the statements directly.** No intermediate rule, so no
intermediate handler. The `*` makes the parameter a list.

**Placement in the choice matters here.** `begin` is a keyword, so no earlier
alternative can match it — but the last alternative is `value:expr ";"`, which
starts with an expression and would happily try. Ordered choice reaches `frame`
first because it is written first. Putting it *after* `evaluate` would still
work (an expression cannot start with `begin`, since `begin` is reserved), but
relying on that is relying on the reservation staying in place.

## 3. Generate

```console
$ nh build pebble.nh -o src/pebble.pest --rust src
ok: generated 9 file(s) in src  [1 new handler(s), 11 kept]
```

```rust
/// * `body` — the `stmt` rule, **unevaluated** — `.eval(host, cx)?` runs it
///   (repeated in the grammar)
pub fn run<H: Handlers>(host: &mut H, body: &[Shared<Stmt>], cx: &mut Ctx)
    -> Result<H::Out>
```

`&[Shared<Stmt>]` — a slice of statements, none of them run. Compare it with
what you would have got **without** `lazy`:

```rust
pub fn run<H: Handlers>(host: &mut H, body: Vec<H::Out>, cx: &mut Ctx)
```

`Vec<H::Out>` — a list of *results*, every statement already executed before
your handler was entered.

## 4. Why `lazy`, when the body runs exactly once anyway

This is the question the chapter exists for. `frame` is not `if`; it does not
skip its body, and it does not repeat it. So why does it care?

Because it needs to do something **around** the body — and "around" is
impossible once the body has already run. Frame redirects output, so it must
be in control at the moment the first `show` happens:

```rust
pub fn run(host: &mut Interp, body: &[Shared<Stmt>], cx: &mut Ctx) -> Result<Value> {
    // Take the output collected so far, so the frame captures only its own.
    let before = std::mem::take(&mut host.output);

    let mut result = Ok(Value::Null);
    for stmt in body {
        result = stmt.eval(host, cx);
        if result.is_err() {
            break;
        }
    }

    let inside = std::mem::replace(&mut host.output, before);
    // ... draw the border around `inside` ...
    result
}
```

With an eager body, `host.output` would already contain the frame's lines mixed
in with everything before it, and there would be no way to tell where the frame
started.

That generalises. **`lazy` is what you need whenever a construct wraps its body
rather than merely containing it** — timing it, catching errors from it,
redirecting its output, running it in a new scope, retrying it, or emitting a
label before it. Control flow is only the most obvious case.

## 5. Errors still propagate

Note the `if result.is_err() { break; }`. A construct that runs statements is
responsible for what happens when one fails. Frame stops at the first error and
returns it — but it still draws the border, because the output produced before
the failure is worth seeing. That is the same reasoning as `main` printing a
partially-recovered run's output in [chapter 10](10-errors.md).

## 6. It works

```pebble
let a = (3, 4);
show a;

begin frame
  show "Pebble";
  show a;
  show 6 * 7;
end frame

show "after";
```

```console
$ cargo run
(3, 4)
+--------+
| Pebble |
| (3, 4) |
| 42     |
+--------+
after
```

## Variations worth trying

* **A named block.** `"begin" name:IDENT lazy body:stmt* "end" IDENT` gives you
  `begin frame … end frame` and `begin group … end group` from one rule, with
  the name as a parameter. Checking that the closing name matches the opening
  one is a *handler's* job, not the grammar's — and a good use of `cx.err`.
* **Repeating it.** `begin frame 3 … end frame` needs only `count:expr` before
  the body and a loop in the handler. The body is already lazy, so nothing else
  changes.
* **Nesting.** The handler above already nests correctly, because it saves and
  restores `host.output` rather than assuming it starts empty:

  ```console
  +-----------+
  | outer     |
  | +-------+ |
  | | inner | |
  | +-------+ |
  | back      |
  +-----------+
  ```

  Nothing in the grammar made that work. It fell out of the body being lazy and
  the handler treating `host.output` as a stack rather than a global.

---

Next: [Changing what you have](09-changing-what-you-have.md).
