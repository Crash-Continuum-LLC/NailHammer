# Build a language, step by step

A short book. By the end you will have written **Pebble** — a small language
with variables, arithmetic that gets precedence right, string concatenation,
`if`/`else`, `while`, and error recovery that reports a mistake and keeps
going — and you will not have written a parser, a precedence climber, or a
single line that walks a parse tree.

It also grows: the last two chapters before the appendix add a **new type** —
a point, `(4, 2)` — and a **new block form**, `begin frame … end frame`,
walking every step of each. Adding to a language you already have is what you
will actually spend your time doing.

The ratio is the point of the exercise: a page of grammar, and a handler per
alternative that mostly fits on a screen.

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

let a = (3, 4);                   # a type you added in chapter 7
begin frame                       # a block form you added in chapter 8
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
| [7. Adding a type](07-a-new-type.md) | A point, `(4, 2)` — and which decisions the compiler forces on you |
| [8. Adding a block form](08-a-new-block.md) | `begin frame … end frame`, and why a body that runs once is still `lazy` |
| [9. When programs are wrong](09-errors.md) | Recovery, better messages, and the determinism lints |
| [10. Choosing a host shape](10-hosts.md) | Interpreter, compiler, or a shared VM — and where to go next |
| [Appendix: Pebble in full](11-pebble-in-full.md) | The finished grammar in one piece, and every handler signature |

## Before you start

```console
$ cargo install nh-cli
$ nh --version
```

You need Rust (1.85 or newer) and about an hour. Each chapter ends with a
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
