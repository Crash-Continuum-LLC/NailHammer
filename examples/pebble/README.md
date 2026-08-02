# Pebble — the language from the guide

The finished result of [`guide/`](../../guide/README.md), which builds this file
by file. Read the book to see *why* each piece is the way it is; read this to
see all of it at once, or to diff against your own attempt.

```console
$ cargo run -p pebble -- examples/pebble/sample.pebble
30
hello, pebble
taller than wide
15
+--------+
| points |
| (4, 6) |
| true   |
+--------+
120
```

## What is here

| Path | | Chapter |
|---|---|---|
| `pebble.nh` | The grammar. The only description of the language | 2–10 |
| `src/lib.rs` | `Value`, `Interp`, frames, and the operator semantics | 4, 6, 7, 9 |
| `src/handlers/` | One file per labelled alternative | 4–10 |
| `tests/run.rs` | Every test runs a program and checks what it printed | 13 |
| `src/main.rs` | Where source comes from and where errors go | — |
| `src/generated/` | Views, dispatch, the owned AST. Never edited | — |

## Why this exists as a crate

The book shows handlers and fragments of `src/lib.rs`, because a chapter that
reprinted the whole host every time would bury the part that changed. That
leaves a gap: `Interp`, `nh_handlers!`, the parser derive and `main.rs` are
referred to but never listed.

This crate is that gap closed. It is also how the book stays true — it is a
workspace member, so CI regenerates it, compiles it, runs clippy over it, and
builds it against `nh-runtime/threadsafe`. A snippet in the book that stopped
matching reality would have to stop matching this first.

## Functions

`fn`, `return`, recursion, and one frame per call so a recursive `fact` cannot
overwrite its caller's variables. `return` is a **signal** — an `Err` that is
not a failure — which is the same mechanism `break` and `continue` would use.

## The two features the book adds after that

**A type.** `(4, 2)` is a `Value::Point`. Chapter 9 walks through every site it
touches, including the three the compiler does *not* make you visit.

**A block form.** `begin frame … end frame` collects everything shown inside and
draws a border round it. Chapter 10 uses it to answer why a body that runs
exactly once still needs `lazy`: because `frame` wraps its body, and "around" is
impossible once the body has already run.

```pebble
begin frame
  show "outer";
  begin frame
    show "inner";
  end frame
  show "back";
end frame
```

```
+-----------+
| outer     |
| +-------+ |
| | inner | |
| +-------+ |
| back      |
+-----------+
```

Nesting was not designed for. It falls out of the body being lazy and the
handler treating `host.output` as a stack rather than a global.
