# 1. A language in ten minutes

Before building anything from scratch, run something that already works. It is
worth seeing the shape of a finished project before you know how the pieces got
there.

```console
$ nh init mylang && cd mylang && cargo run
--- bytecode ---
  0  LoadK { dst: 0, value: 4.0 }
  1  StoreGlobal { name: "width", src: 0 }
  ...
--- output ---
28
22
5
...
```

That is a working language: variables, arithmetic with real precedence,
branches, three kinds of loop with `break`/`continue`, functions with recursion,
and error recovery.

It **compiles to bytecode** and runs it, which is why the disassembly comes
first. `nh init --interpreter` scaffolds a tree-walking interpreter instead, and
the two print the same numbers from the same grammar. Chapter 8 covers the
choice; ignore it for now.

## What is in there

```
mylang.nh              the grammar — the only description of your language
src/handlers/**.rs     yours: one small file per grammar alternative,
                       grouped into a directory per rule
src/lib.rs             yours: the value type and operator semantics
src/main.rs            yours: where source comes from, where errors go
src/generated/**       generated: views, dispatch, diagnostics — never edit
```

The split is the whole design. Everything under `generated/` is a consequence
of the grammar. Everything else is a decision only you can make.

## Add a statement

Open `mylang.nh`, find `rule stmt`, and add an alternative:

```nh
  | "twice" value:expr ";"   -> twice
```

Then rebuild:

```console
$ cargo run
warning: mylang@0.1.0: ok: wrote src/mylang.pest
warning: mylang@0.1.0: ok: generated 10 file(s) in src  [0 new handler(s), 21 kept]
warning: mylang@0.1.0: note: 1 handler(s) not implemented yet — each reports at run time
```

Three things happened without you asking:

1. A handler file appeared at `src/handlers/stmt/twice.rs` — under `stmt`,
   because that is the rule the alternative belongs to.
2. It already knows its parameter is called `value` and arrives **evaluated**.
3. The build **counted it**, and the stub says so if a program reaches it:

```console
$ cargo run -- t.mylang
error: `stmt_twice` is not implemented yet
 --> t.mylang:1:1
  |
1 | twice 21;
  | ^^^^^^^^^
```

The rest of the language still runs. That matters more than it looks: a stub
that refused to *compile* would mean no `cargo run` at all until the last one
was written, and the handlers for conditionals and loops cannot be written until
every other handler already is.

You did not register the handler anywhere. The file's *name* is the
registration, and `nh build` wrote the dispatch that calls it.

## Syntax is not semantics

Now change the keyword `print` to `show` — the literal in the grammar, and its
entry in the `reserved from` list:

```nh
  | "show" value:expr ";"    -> print
```

```console
$ cargo run
28
22
...
```

It works immediately, and `src/handlers/stmt/print.rs` was not touched. The
handler is named by the **label** (`-> print`), not by the spelling. Changing
what a language looks like does not disturb what it means — which is the
property that lets `examples/vm-c` and `examples/vm-basic` be the same language
in two syntaxes.

## Renaming a binding

Change `value:expr` to `amount:expr` and rebuild:

```console
$ cargo run
warning: handlers/stmt/print.rs names its parameters differently than the grammar binds them
  grammar:  amount
  handler:  value
help: rename the parameters to match, so the handler says what it reads
```

A **warning**, not an error — the parameters still line up by position, so the
code is still correct. What it is telling you is that the handler has stopped
describing itself honestly. Rename the parameter and the warning goes.

> This check exists because the scaffold itself once violated it. Two of its
> own handler templates named a parameter `a` where the grammar bound `first`,
> so a brand-new project greeted its author with two warnings about code the
> author had not written. `the_scaffold_does_not_ship_parameter_drift` now
> fails the build if that comes back.

## The loop

```console
$ $EDITOR mylang.nh          # 1. change the grammar
$ nh check mylang.nh         # 2. fast feedback, nothing compiled
$ cargo run                  # 3. build.rs regenerates, then runs
```

`nh check` runs the same analysis `build` does, so it will not pass a grammar
that `build` would reject.

---

Next: [Tokens, and a rule that holds a program](02-tokens-and-rules.md) — where
we set this aside and start Pebble from an empty file.
