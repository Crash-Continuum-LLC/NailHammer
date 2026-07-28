# NailHammer
A metaphorical hammer built specifically for the metaphorical nail you are trying to (maybe metaphorical or not) hit...

Write a grammar. Get a parser, typed accessors, and one small handler file per
rule — with operator precedence, error recovery, and determinism checks you did
not have to write.

```console
$ nh init mylang && cd mylang && cargo run
28
22
5
14
```

That is a working interpreter: variables, arithmetic with real precedence,
assignment, and error recovery. Six handler files of a few lines each.

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

token NUMBER = @ DIGIT+ ("." DIGIT+)?;
token IDENT  = @ (ALPHA | "_") (ALPHA | DIGIT | "_")*;

reserved from IDENT { "let" "print" }

rule program = SOI stmts:stmt* EOI -> doc;

rule stmt
  = "let" name:IDENT "=" value:expr ";" -> bind
  | "print" value:expr ";"              -> print
  ;

recover stmt sync ";";
```

```rust
// handlers/stmt_bind.rs — one file per alternative
pub fn run(host: &mut Interp, name: &str, value: Value, cx: &mut Ctx) -> Result<Value> {
    host.vars.insert(name.to_string(), value.clone());
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

Install the tool once:

```console
$ cargo install --git https://github.com/Crash-Continuum-LLC/NailHammer nh-cli
```

That puts `nh` in `~/.cargo/bin`. The repository is private, so it needs an
account with access and one cargo setting — cargo's built-in git client cannot
use `gh`'s credential helper:

```toml
# ~/.cargo/config.toml
[net]
git-fetch-with-cli = true
```

**Or skip the build entirely** and take a prebuilt binary. Releases carry `nh`
for macOS (arm64 and x86_64), Linux, and Windows:

```console
$ gh release download v0.1.0 --repo Crash-Continuum-LLC/NailHammer \
    --pattern '*macos-arm64*'
$ tar xzf nh-macos-arm64.tar.gz
$ sudo mv nh-macos-arm64/nh /usr/local/bin/
```

Then:

```console
$ nh init mylang
$ cd mylang
$ cargo run
28
22
5
14
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

Read [USAGE.md](USAGE.md) for the language reference, and
[DESIGN.md](DESIGN.md) for why it is built this way — including a running
record of what went wrong and what that taught.

## Editor support

```console
$ gh release download v0.1.0 --repo Crash-Continuum-LLC/NailHammer --pattern '*.vsix'
$ code --install-extension nailhammer-0.1.0.vsix
```

Highlighting, live diagnostics in the Problems panel, `nh init` from the command
palette, and tasks for check/build/explain. It shells out to `nh check --json`,
so the lint you see in the editor is the lint CI runs.

Or build it from source with `cd editors/vscode && npm run package`. See
[editors/vscode](editors/vscode).

## Examples

| | |
|---|---|
| [`examples/config`](examples/config) | A config-language interpreter. Nine handlers, two or three lines each |
| [`examples/calc-interp`](examples/calc-interp) | Operators end to end: precedence, short-circuiting, assignment, recovery, `lazy` |
| [`examples/selfhost`](examples/selfhost) | `.nh` describing `.nh`, parsing every grammar in this repo |
| [`examples/basic-interp`](examples/basic-interp) | Mini BASIC: `PRINT`, `FOR`, `WHILE`, `SUB`, `FUNCTION`, `GOTO`, `EXIT`/`CONTINUE`. Recursion with local frames, stored bodies, jumps, signals, a from-scratch operator table |
| [`examples/bytecode`](examples/bytecode) | A **bytecode compiler**, not an interpreter. Same handler shapes, `type Out = ()`, handlers emit instead of compute. Precedence becomes instruction order; `lazy` becomes jump patching |
| [`examples/basic.nh`](examples/basic.nh) | The BASIC grammar on its own, with `GOTO` and line numbers |

## Status

Every planned milestone is complete: parsing, lowering, code generation, the
operator driver, determinism analysis, error recovery, self-hosting, and an
owned AST that makes subroutines, stored code, and non-local jumps
expressible. 317 tests.

**Not published, and it does not need to be.** `nh init` vendors the runtime
into the project it creates, so a generated project depends on pest and nothing
else — no credentials, no cargo configuration, no registry. Install the tool
with `cargo install --git`, or take a prebuilt binary from a release. See
[PUBLISHING.md](PUBLISHING.md).

Known gaps are tracked in [DESIGN.md §11](DESIGN.md), openly — including the
ones found by using the tool on itself.

## License

MIT. See [LICENSE](LICENSE).
