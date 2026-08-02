# Build a language, step by step

A short book. By the end you will have written **Pebble** — a small language
with variables, arithmetic that gets precedence right, string concatenation,
`if`/`else`, `while`, functions with recursion, a type of its own, a block form
of its own, and error recovery that reports a mistake and keeps going — and you
will not have written a parser, a precedence climber, or a single line that
walks a parse tree.

The chapters come in four movements: **build** it (1–7), **extend** it (8–10),
**maintain** it (11–13), and **ship** it (14). The ratio is the point of the
exercise: a page of grammar, and a handler per alternative that mostly fits on
a screen.

```pebble
let width = 4;
let height = 7;
show width * height + 2;          # 30 — not 44

let name = "pebble";
show "hello, " + name;            # hello, pebble

let n = 1;
let total = 0;
while n <= 5 {
  total = total + n;
  n = n + 1;
}
show total;                       # 15

fn fact(n) {                      # functions, chapter 7
  if n < 2 { return 1; }
  return n * fact(n - 1);
}
show fact(5);                     # 120

let a = (3, 4);                   # a type you added, chapter 9
begin frame                       # a block form you added, chapter 10
  show a + (1, 2);                # (4, 6)
end frame
```

## The chapters

| | |
|---|---|
| [1. A language in ten minutes](01-ten-minutes.md) | `nh init`, run it, change it. A win before any theory |
| [2. Tokens, and a rule that holds a program](02-tokens-and-rules.md) | Starting Pebble from an empty file |
| [3. Expressions you do not write](03-expressions.md) | The operator table, `atom`, and roles |
| [4. Handlers are your language](04-handlers.md) | Bindings arrive as parameters. This is the chapter that matters |
| [5. Control flow, and what `lazy` is for](05-control-flow.md) | `if`, `while`, and why a condition is sometimes not a value |
| [6. Assignment, and `place`](06-assignment.md) | The one enum the grammar generates for you |
| [7. Functions](07-functions.md) | Parameters as names, frames, and `return` as a signal |
| [8. Case, and what a name is](08-case.md) | Case-insensitive languages, `.key()` and `.text()` |
| [9. Adding a type](09-a-new-type.md) | A point, `(4, 2)` — and which decisions the compiler forces on you |
| [10. Adding a block form](10-a-new-block.md) | `begin frame … end frame`, and why a body that runs once is still `lazy` |
| [11. Changing what you have](11-changing-what-you-have.md) | Renaming, removing, `--prune`, and splitting a grammar |
| [12. When programs are wrong](12-errors.md) | Recovery, better messages, and the determinism lints |
| [13. Testing your language](13-testing.md) | Through the front door, and a test that found a real bug |
| [14. Choosing a host shape](14-hosts.md) | Interpreter, compiler, or a shared VM — and where to go next |
| [Appendix: Pebble in full](15-pebble-in-full.md) | The finished grammar in one piece, and every handler signature |

## What this book does not cover

Two constructs Pebble had no use for, both a section of
[USAGE.md](../USAGE.md) away: **silent rules** (`silent rule` matches without
producing a node, and cannot be bound) and **`boundary`** (fine control over
what counts as the end of a keyword).

Everything else you need to build a language is in here.

## Before you start

```console
$ cargo install nh-cli
$ nh --version
```

You need Rust (1.85 or newer) and an afternoon. Each chapter ends with a
program that runs, so stopping early still leaves you something working.

If you would rather read reference material than build something,
[USAGE.md](../USAGE.md) documents every construct, and
[DESIGN.md](../DESIGN.md) is the argument behind each of them. This book
deliberately does not repeat either; it takes one path through the middle and
links out where a detail deserves more room.

## A note on how this is written

Every grammar in this book is checked by `nh check` in CI, so a snippet that
stopped being valid would fail the build rather than sit here misleading you.
Where a chapter says "this fails", it fails — the message quoted is the message
you get.
