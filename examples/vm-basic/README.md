# `vm-c` and `vm-basic` — two syntaxes, one machine

The same language written twice: once with braces and `&`, once with
`END IF` and `AND`. They compile to **identical bytecode**, and neither
contains a line of operator code.

```console
$ cargo run -p vm-c
$ cargo run -p vm-basic
```

Both print `18 -6 true 8 1 0 1 2` — the same values from the same count of
instructions, which `tests/agree.rs` pins.

## What is generated

One line of `build.rs` does it:

```rust
nh_build::Builder::new("lang.nh").target("nh-vm").run();
```

`--target` makes NailHammer write `src/generated/vm_operators.rs` — the whole
`Operators` implementation. Against a machine that owns execution, `add` means
`Op::Add` in every language, so the body is a consequence rather than a
decision (VM-DESIGN.md §7.2).

Search either crate for `fn add`, `Op::Mul`, or a comparison `match`. There
isn't one.

## What is not

The grammars, and the statement handlers. Compare `lang.nh` side by side:

| | `vm-c` | `vm-basic` |
|---|---|---|
| block | `{ … }` | `THEN` … `END IF` |
| and | `&` | `AND` |
| assign | `x = 1;` | `LET x = 1` |
| terminator | `;` | newline |

Not a line in common — and the **roles are identical**, which is why `&` and
`AND` both become `Op::And`. Roles are about meaning, not spelling
(DESIGN.md §6).

## Why `lazy` appears twice in `while`

```
| "while" "(" lazy cond:expr ")" lazy body:block -> whilst
```

A loop re-tests every iteration, so the condition's code belongs at the top of
the loop — which means the handler has to know where the top *is* before the
condition is emitted. An eager `cond` would already be behind us. `if` needs it
only for the body, to put a jump in front.

## The tests that matter

`tests/agree.rs` checks the two produce the same output, the same register
frame, and the same instructions **in the same order**. Two grammars sharing no
syntax, making the same lowering decisions, because the decisions were never
theirs to make.

Agreement is easy to keep by accident once one twin stops growing, so two tests
guard against that directly:

* `the_pair_covers_the_whole_language` reads both grammar files and fails,
  naming the feature, when a construct exists in one and not the other. It
  anchors on the text that *defines* each construct — an earlier version matched
  bare keywords, and a keyword left behind in a `reserved from` list was enough
  to satisfy it while the languages had already diverged.
* `the_twins_agree_on_a_program_that_uses_everything` runs one program through
  both using recursion, `else`, arrays, indexed assignment, strings, `len`,
  `while`, bitwise operators and short-circuit — comparing instruction for
  instruction, and asserting the answers, so "equal" cannot quietly mean
  "equally wrong".
