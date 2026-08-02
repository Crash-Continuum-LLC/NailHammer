# NailHammer
A metaphorical hammer built specifically for the metaphorical nail you are trying to (maybe metaphorical or not) hit...

> ### ⚠︎ Under active development
>
> **Things can and will move.** The grammar language, the generated APIs, and
> the CLI are all still changing, sometimes in ways that will not be quietly
> compatible. Recent releases have renamed traits, changed what a handler
> receives, and altered what `nh init` scaffolds by default.
>
> What that means in practice:
>
> * **Pin a release** and upgrade deliberately, rather than tracking `main`.
>   `nh init` vendors the runtime, so a generated project keeps working against
>   the `nh` that made it even after the toolkit moves on.
> * **Expect to re-run `nh build`** after upgrading, and to fix compile errors in
>   your handlers. That is by design — a grammar change that alters what a
>   handler receives should stop the build, not misbehave at run time — but it
>   applies to *toolkit* changes too.
> * **Read the release notes.** They say what moved and why.
>
> The design record in [DESIGN.md](DESIGN.md) is kept honestly, including the
> decisions that were reversed. If something reads as settled there, it probably
> is; if it reads as recently argued with, it may move again.

Write a grammar. Get a parser, typed accessors, and one small handler file per
rule — with operator precedence, error recovery, and determinism checks you did
not have to write.

```console
$ nh init mylang && cd mylang && cargo run
--- bytecode ---
  0  LoadK { dst: 0, value: 4.0 }
  1  StoreGlobal { name: "width", src: 0 }
  ...
 99  Print { src: 3 }
--- output ---
28
22
5
14
1
3
2
1
10
20
40
99
42
120
```

That is a working language: variables, arithmetic with real precedence,
branches, three kinds of loop with `break`/`continue`, functions with recursion,
and error recovery — in handler files of a few lines each.

It compiles to bytecode and runs it, which is why the disassembly comes first.
`nh init --interpreter` scaffolds a tree-walking interpreter instead; both
shapes share the same grammar and print the same numbers.

Run it at a prompt and it asks two questions — which syntax, and what to
include — defaulting to exactly the above. In a script it takes those defaults
without asking, so the same command builds the same project either way. `--with
none` gives you the thin version.

## The problem

Writing an interpreter on a PEG library means three kinds of tedium, and one
real hazard:

- **Operator precedence by hand.** PEGs forbid left recursion, so binary
  operators become a ladder of `term`/`factor`/`unary` rules that is verbose and
  easy to get subtly wrong.
- **Positional access to the parse tree.** `pair.into_inner().nth(2)` compiles
  no matter what the grammar says. Reorder a rule and it silently reads the
  wrong node.
- **One giant file.** Coverage of a real AST tends to collapse into a single
  unmaintainable `match`.
- **Ordered choice that lies.** `"let" | "letter"` never matches `letter`, and
  nothing in the grammar text says so.

## What NailHammer does

You write `.nh`. It generates the `.pest` grammar and the Rust that surrounds
it.

```nh
grammar Calc;

use operators::core;          // supplies `expr`, precedence, short-circuiting

skip WHITESPACE = " " | "\t" | "\r" | "\n";

token DIGIT  = @ "0".."9";
token ALPHA  = @ "a".."z" | "A".."Z";
token NUMBER = @ DIGIT+ ("." DIGIT+)?;
token IDENT  = @ (ALPHA | "_") (ALPHA | DIGIT | "_")*;

reserved from IDENT { "let" "print" }

rule program = SOI stmts:stmt* EOI -> doc;

rule stmt
  = "let" name:IDENT "=" value:expr ";" -> bind
  | "print" value:expr ";"              -> print
  ;

// `atom` is what the operator table folds expressions out of.
rule atom
  = value:NUMBER       -> num
  | name:IDENT         -> var place
  | "(" inner:expr ")" -> pass
  ;

recover stmt sync ";";
```

```rust
// handlers/stmt_bind.rs — one file per alternative
pub fn run(host: &mut Interp, name: &str, value: Value, cx: &mut Ctx) -> Result<Value> {
    host.set(name, value.clone());
    Ok(value)
}
```

The parameters *are* the bindings: a token arrives as text, a sub-rule arrives
already evaluated, and the generated evaluator did the walking. There is no
parse tree in a handler.

| | |
|---|---|
| **No tree walking** | Bindings arrive as parameters, evaluated. Reorder the grammar and handlers do not change; rename a binding and they stop compiling |
| **Cardinality in the type** | `stmts:stmt*` gives `Vec`, `x:y?` gives `Option`. Change `*` to `?` and the handler stops compiling |
| **`lazy` when you need it** | `lazy body:stmt` hands the handler something unevaluated and **owned**, so an `if` can decline to run its body — and a `SUB` can keep it and run it later |
| **Missing handlers break the build** | Add an alternative and you get a `compile_error!` naming the file to write |
| **Operators are free** | One `use operators::core;` supplies precedence, associativity, and short-circuiting. `&&` does not evaluate its right operand |
| **Assignment is correct** | `a[f()] += 1` calls `f()` exactly once, because a place is resolved before the read and the write |
| **Errors locate themselves** | `cx.err("...")` already knows which node it is inside |
| **Recovery reports everything** | One bad statement does not hide the rest |
| **Determinism is checked** | Left recursion, nullable repetitions, and shadowed alternatives are reported with fixes |

## Seeing where a program goes

`nh trace` answers "which handler gets this, and what does it receive?" without
generating or compiling anything — `pest_vm` interprets your grammar, so it is as
fast as parsing:

```console
$ nh trace mylang.nh --source 'if x > 1 { print 2 + 3 * 4; }'
program  → handlers/program.rs
  · SOI stmts:stmt* EOI -> doc
  stmts: Vec<Self::Out>   ⟵ evaluated first, by:
    stmt_iff  → handlers/stmt_iff.rs
      · "if" cond:expr lazy then:block lazy otherwise:else_tail? -> iff
      cond: Self::Out   ⟵ evaluated first, by:
        Operators::compare
          · `>` — left-associative, precedence 4
          lhs: Self::Out   ⟵ evaluated first, by:
            primary_var  → handlers/primary_var.rs
              · name:IDENT -> var place
              name: &str = "x"
          ...
      then: &Shared<Block>   ⟵ lazy: the node, unevaluated
        ...
      otherwise: Option<&Shared<ElseTail>>   ⟵ absent here
```

Children hang off the **argument** they produce, `lazy` parameters are marked as
the one case where what is below has not run yet, and operators are folded the
way the driver folds them — which nothing else can show you, because precedence
lives in the operator table rather than in the grammar.

`--json` gives the same tree as data. The VS Code extension puts it in a live
pane.

## Determinism analysis

The reason the project exists. `nh check` reports the cases where a PEG means
something other than it looks like — and only when it is *certain*, because a
warning that fires on working code is one you learn to ignore.

```console
$ nh check mylang.nh
warning: this alternative is unreachable: an earlier one matches `let`,
         which is a prefix of `letter`
 --> mylang.nh:8:28
  |
8 | rule kw = "let" -> short | "letter" -> long;
  |                            ^^^^^^^^^^^^^^^^
help: ordered choice takes the first match, so put the longer alternative first
```

Eight lints; five are errors because each means the grammar cannot work. `--deny-warnings`
for CI, `allow <lint> in <rule>;` for the deliberate cases.

## Getting started

With Rust already installed:

```console
$ cargo install nh-cli
```

Without it — `install.sh` takes a prebuilt `nh` from the latest release and puts
it in `~/.local/bin`, so it needs no toolchain at all:

```console
$ curl -fsSL https://raw.githubusercontent.com/Crash-Continuum-LLC/NailHammer/main/install.sh | bash
```

It falls back to `cargo install` when there is no prebuilt binary for the
platform, and tells you what to add to your PATH if `~/.local/bin` is not on it.
`--prefix DIR` to install somewhere else, `--version TAG` to pin a release,
`--from-source` to always build, `--help` for the rest.

Or take a prebuilt binary by hand. Releases carry `nh` for macOS (arm64 and
x86_64), Linux, and Windows — the last of which `install.sh` does not cover:

```console
$ curl -fsSL -O https://github.com/Crash-Continuum-LLC/NailHammer/releases/latest/download/nh-macos-arm64.tar.gz
$ tar xzf nh-macos-arm64.tar.gz
$ sudo mv nh-macos-arm64/nh /usr/local/bin/
```

`/releases/latest/download/` rather than a version, so this does not quietly
become instructions for an old release the way a named tag does.

Then:

```console
$ nh init mylang        # asks which syntax, and what to include
$ cd mylang
$ cargo run
```

Then edit `mylang.nh`. The scaffolded project has a `build.rs`, so `cargo build`
regenerates on its own.

That project depends on **pest and nothing else** — `nh init` vendors the
runtime into it. So you can hand it to somebody, or build it in CI, without
installing anything: `nh` is needed to *change* the grammar, not to build what
it produced. Edit the `.nh` without `nh` available and the build stops and says
so, rather than quietly compiling the previous grammar.

> **Working on NailHammer itself?** Use `cargo run -p nh-cli -- <args>` from a
> clone instead, or `cargo install --path crates/nh-cli` to put your working
> copy on the `PATH`.

**New here?** [**guide/**](guide/README.md) is a short book that builds a small
language from an empty file: tokens, expressions you do not write, handlers,
control flow, assignment, and error recovery. About an hour, and each chapter
ends with something that runs.

Then [USAGE.md](USAGE.md) for the language reference, and
[DESIGN.md](DESIGN.md) for why it is built this way — including a running
record of what went wrong and what that taught.

## Editor support

```console
$ cd editors/vscode && npm test && npx @vscode/vsce package --no-dependencies
$ code --install-extension nailhammer-*.vsix
```

Highlighting, completion, live diagnostics in the Problems panel, `nh init` from
the command palette, tasks for check/build/explain, and an **evaluation
playground**: a pane beside your grammar where you type a program and watch
where it routes, updated as you type. It shells out to `nh check --json` and
`nh trace --json`, so what the editor shows is what the CLI and CI compute.

The `.vsix` is attached to each release alongside the binaries.

See [editors/vscode](editors/vscode).

## Examples

| | |
|---|---|
| [`examples/config`](examples/config) | A config-language interpreter. Nine handlers, two or three lines each |
| [`examples/calc-interp`](examples/calc-interp) | Operators end to end: precedence, short-circuiting, assignment, recovery, `lazy` |
| [`examples/pebble`](examples/pebble) | The language the [guide](guide/README.md) builds, finished |
| [`examples/selfhost`](examples/selfhost) | `.nh` describing `.nh`, parsing every grammar in this repo |
| [`examples/basic-interp`](examples/basic-interp) | Mini BASIC: `PRINT`, `FOR`, `WHILE`, `SUB`, `FUNCTION`, `GOTO`, `EXIT`/`CONTINUE`. Recursion with local frames, stored bodies, jumps, signals, a from-scratch operator table |
| [`examples/bytecode`](examples/bytecode) | A **bytecode compiler**, not an interpreter. `type Out = ()`, a stack machine, handlers emit instead of compute. Precedence becomes instruction order; `lazy` becomes jump patching |
| [`examples/basic.nh`](examples/basic.nh) | The BASIC grammar on its own, with `GOTO` and line numbers |

## Two shapes from one grammar

An interpreter and a compiler are two impls over the same `.nh`, and the
difference is one line:

```rust
type Out = Value;   // an interpreter: what a node evaluated to
type Out = Reg;     // a compiler: which register holds the result
```

**The compiler is the default.** `nh init` scaffolds a **register machine**
emitting three-address code, with locals allocated to slots at compile time; the
operator trait reads as three-address code without any change to the toolkit:

```rust
fn add(&mut self, a: Reg, b: Reg) -> Result<Reg>
```

`nh init --interpreter` gives you the tree-walker instead — shorter to read, and
the quicker path to a working language. An `#[ignore]`d end-to-end test builds
all sixteen style × feature × shape combinations and asserts the two shapes print
the same thing.

### Why the compiler is the default

It is the shape that scales, and it is the only one that can **suspend**. If your
language grows `await`, a compiled host stops and lets its driver do the waiting:

```rust
Step::Awaiting(handle) => m.resume_with(resolve(handle).await),
```

Nothing in the generated machine mentions a runtime, a future, or a thread, so the
same bytecode is driven by a blocking loop, a multi-thread host, or a
single-threaded one — and the last is where "just block on it" panics.

A tree-walker cannot do that. Blocking on a future needs a multi-thread runtime,
costs a worker thread per await, and stalls every other program if the language
has concurrency of its own; an async evaluator boxes a future per AST node whether
a language awaits or not. **So nothing is scaffolded for it.** Wiring it yourself
is perfectly possible, and different from being handed it.

## Threads

Nothing generated starts a thread, picks a runtime, or assumes how many of either
you have. The one place it could have decided for you is the AST's shared
pointer, and that is a feature:

```toml
nh-runtime = { path = "vendor/nh-runtime", features = ["threadsafe"] }
```

Default is `Rc` — cheap, single-threaded. With the feature it is `Arc`, and a
program tree is `Send + Sync`, so you can parse on one thread and run on another,
or share a stored function body between workers. **No signatures change**:
generated code and your handlers both say `Shared<T>`.

Off by default because a single-threaded interpreter should not pay for atomics
it never needs — and on-by-default would have been a dictate in the other
direction. `USAGE.md` covers what it does not do, and what a compiled language
does instead when it needs to await.

## Status

Every planned milestone is complete: parsing, lowering, code generation, the
operator driver, determinism analysis, error recovery, self-hosting, an owned AST
that makes subroutines, stored code and non-local jumps expressible, both host
shapes, and `nh trace`.

**Complete is not the same as settled.** The milestones are done; the interfaces
are not frozen. Most of what has moved recently moved *because* the tool was used
to build something and the design turned out to be wrong — that is the intent,
and it is why the warning at the top is there.

**On crates.io, and a generated project still does not need it.** `nh init`
vendors the runtime into the project it creates, so what you scaffold depends on
pest and nothing else — no registry, no cargo configuration, no access to this
repository. Publishing changed how you get `nh`, not what your project carries.
See [PUBLISHING.md](PUBLISHING.md).

Known gaps are tracked in [DESIGN.md §11](DESIGN.md), openly — including the
ones found by using the tool on itself.

## License

MIT. See [LICENSE](LICENSE).
