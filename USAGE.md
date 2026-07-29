# Using NailHammer

This guide is for people building a language with NailHammer. It explains what
to write and why. For the reasoning behind the design, see
[DESIGN.md](DESIGN.md) — that is a different document for a different purpose.

**Contents**

1. [Start here](#start-here)
2. [Your first grammar](#your-first-grammar)
3. [Grammar reference](#grammar-reference)
4. [Operators](#operators)
5. [Running a program](#running-a-program)
6. [Starting a project](#starting-a-project)
7. [Two shapes: interpreter and compiler](#two-shapes-interpreter-and-compiler)
8. [Writing handlers](#writing-handlers)
9. [Control flow](#control-flow)
10. [Errors and recovery](#errors-and-recovery)
11. [Threads, and what is not decided for you](#threads-and-what-is-not-decided-for-you)
12. [Seeing where a program goes](#seeing-where-a-program-goes)
13. [Checking your grammar](#checking-your-grammar)
14. [Reading the generated `.pest`](#reading-the-generated-pest)
15. [Known gaps](#known-gaps)

---

## Start here

### Getting `nh`

```console
$ cargo install --git https://github.com/Crash-Continuum-LLC/NailHammer nh-cli
$ nh --version
nh 0.1.0
```

This puts `nh` in `~/.cargo/bin`, which `rustup` already adds to your `PATH`.
Because the repository is private you also need one cargo setting, since
cargo's built-in git client cannot use `gh`'s credential helper:

```toml
# ~/.cargo/config.toml
[net]
git-fetch-with-cli = true
```

Without it you get `failed to acquire username/password`, which does not say
what to do about it.

To skip the Rust toolchain entirely, take a prebuilt binary from a release.
Builds exist for macOS (arm64 and x86_64), Linux, and Windows:

```console
$ gh release download v0.2.0 --repo Crash-Continuum-LLC/NailHammer \
    --pattern '*macos-arm64*'
$ tar xzf nh-macos-arm64.tar.gz
$ sudo mv nh-macos-arm64/nh /usr/local/bin/
```

You still need cargo to *build* the project `nh init` creates — it is a Rust
program — but not to run the tool itself.

### Your first project

Run `nh init`. It creates a project that already works:

```console
$ nh init mylang
$ cd mylang
$ cargo run
28
22
5
14
```

That is a real interpreter — variables, arithmetic with correct precedence,
printing, assignment, and error recovery. It is not a skeleton you have to fill
in before anything runs.

Two settings in that project fail *silently* if you get them wrong: anchoring
the entry rule, and enabling `pest_derive`'s `grammar-extras` feature. The
scaffold sets both. You do not need to know about either yet.

Then edit `mylang.nh` and loop:

```console
$ nh check mylang.nh                                  # fast feedback
$ nh build mylang.nh -o src/mylang.pest --rust src    # regenerate
$ cargo run
```

The scaffold includes a `build.rs`, so the middle step is optional — `cargo
build` regenerates whenever the grammar changes.

`nh build --rust` never overwrites a handler you have written. New grammar
alternatives get a stub; your existing files are left alone.

### Commands

```
nh init    [dir] [--name <name>] [--ext <ext>] [--force]
nh check   <file.nh> [--quiet] [--deny-warnings] [--json]
nh check   --lints
nh build   <file.nh> [-o <out.pest>] [--rust <src-dir>] [--prune [--force]]
nh explain <file.nh> [--source]
```

**`nh init`** writes into `dir`, defaulting to the current directory. It refuses
a non-empty directory unless you pass `--force`. The project name comes from the
directory name; `--ext` sets the file extension for your language's source files. `--async` adds tokio and a helper for awaiting from inside a handler — see
[Async work in a handler](#async-work-in-a-handler).

**`nh check`** parses and reports without writing anything. It runs lowering
internally and throws the result away, so it will not accept a grammar that
`build` would reject. `--json` prints the diagnostics as a JSON array on stdout
and nothing else, for editors and other tools.

**`nh build`** writes the `.pest` next to your `.nh` unless `-o` says otherwise.
Add `--rust <src-dir>` to also generate the AST, the evaluator, and handler
stubs.

**`nh explain`** prints the resolved operator table:

```console
$ nh explain examples/calc.nh
preset: operators::c_style

 13  = += -= *= /= %= <<= >>= &= ^= |=  right   lazy(lhs)  -> assign
 12  |>                                 left    lazy(rhs)  -> pipe
 11  ||                                 left    lazy(rhs)  -> or_else
  ...
  2  **                                 right    -> pow
  1  ! ~ - +                            prefix   -> not, bit_not, neg, pos

atom: `atom`
```

The driver derives precedence from the same table, so what this prints is what
runs. `--source` prints a preset as `.nh` you could paste into your own grammar.

Exit status is `0` on success, `1` for a grammar error, `2` for a usage mistake.
`--quiet` makes `check` usable as a CI gate.

### In your editor

```console
$ cd editors/vscode && npm run package
$ code --install-extension nailhammer-*.vsix
```

The VS Code extension highlights `.nh`, completes declarations and roles, puts
`nh check`'s diagnostics in the Problems panel as you type, and scaffolds a
project from the command palette.

It also carries an **evaluation playground**: a pane beside your grammar where
you type a program and watch which handler each part of it reaches, updated as
you type. That is `nh trace` — see
[Seeing where a program goes](#seeing-where-a-program-goes) — so what the editor
shows is what the CLI computes.
Point `nailhammer.executable` at your `nh` binary if it is not on `PATH`.

Diagnostics come from `nh check --json`, which is also worth knowing about on
its own — it prints a JSON array on stdout and nothing else, so any tool can
consume it:

```console
$ nh check mylang.nh --json
[{"severity":"warning","message":"...","code":"shadow","location":{...},"help":"..."}]
```

### Examples in this repository

| File | What it shows |
|---|---|
| `example.nh` | The smallest useful grammar — a calculator |
| `examples/calc.nh` | Imports, suffix chains, `place`, recovery, `expect` |
| `examples/config/` | A complete interpreter. Nine handlers, two or three lines each |
| `examples/calc-interp/` | Operators end to end, proved by tests |
| `examples/basic-interp/` | Mini BASIC: loops, subroutines, functions, `GOTO` |
| `examples/bytecode/` | The same idea compiled instead of interpreted — `type Out = ()`, a stack machine |
| `examples/selfhost/` | `.nh` describing `.nh` |

---

## Your first grammar

```nh
grammar Example;

use operators::core;

skip WHITESPACE = " " | "\t" | "\r" | "\n";

token DIGIT  = @ "0".."9";
token ALPHA  = @ "a".."z" | "A".."Z";
token NUMBER = @ DIGIT+ ("." DIGIT+)?;
token IDENT  = @ (ALPHA | "_") (ALPHA | DIGIT | "_")*;

reserved from IDENT { "let" }

rule program = SOI stmt* EOI;

rule stmt
  = "let" name:IDENT "=" value:expr ";" -> let
  | value:expr ";"                      -> eval
  ;

rule atom = primary;

rule primary
  = value:NUMBER       -> num
  | name:IDENT         -> var place
  | "(" inner:expr ")" -> pass
  ;
```

Two things in there are worth explaining before you go further.

**There is no `expr` rule.** You did not forget it. `use operators::core` supplies
`expr`, along with precedence, associativity, and short-circuit behaviour. What
your grammar supplies is `atom` — the rule the operator machinery builds
expressions out of. Writing operator grammars by hand is the tedium NailHammer
exists to remove.

**`SOI` and `EOI` anchor the entry rule.** Whitespace is skipped *between*
elements, never before the first one. Without `SOI`, a program that starts with a
blank line or a comment will not parse. Only you know which rule is the entry
point, so this is your job — the same as it is in hand-written pest.

> **This one is worth getting right early, because it fails selectively.** An
> unanchored grammar rejects a leading blank line only when the first statement
> begins with a keyword. `\n1 + 1;` parses anyway, because `expr` starts with a
> repetition and pest skips whitespace around those — so the same grammar
> accepts one program and rejects the next. Two grammars in this repository
> shipped unanchored for exactly that reason, and
> `crates/nh-lower/tests/anchoring.rs` now checks every one of them.

---

## Grammar reference

### Declarations

```nh
grammar Name;                  // exactly one across all imported files
import "path/to/other.nh";     // relative to the importing file
use operators::c_style;        // c_style | c_strict | core | none
keywords case-insensitive;     // or case-sensitive, which is the default
```

### Tokens and skipping

```nh
skip WHITESPACE = " " | "\t";
skip COMMENT    = "//" (!"\n" ANY)*;

token NUMBER = @ DIGIT+ ("." DIGIT+)?;
token IDENT  = @ case-insensitive ALPHA (ALPHA | DIGIT | "_")*;
```

`skip` definitions are matched implicitly between other elements.

**A token never skips whitespace**, with or without `@`. So
`token W = "<" X ">";` does not match `< abc >`. What `@` changes is whether the
token's internals survive into the tree:

| | |
|---|---|
| `token X = @ body;` | **Atomic.** No inner nodes. Use this for anything lexical |
| `token X = body;` | **Compound-atomic.** Inner rules still produce nodes, so you can reach inside the token |

### Case folding

There are two independent switches, and you will want different combinations for
different languages:

- `case-insensitive` on a **token** makes that token fold.
- `keywords case-insensitive` makes **literals**, word operators, and the
  reserved-word guard fold.

| Language | `keywords` | token |
|---|---|---|
| BASIC, Pascal | insensitive | insensitive |
| SQL | insensitive | sensitive |
| C, Rust | *(omit)* | *(omit)* |

> Folding is **ASCII-only**. Pest compares byte-length-matched slices with
> `eq_ignore_ascii_case`, so Unicode pairs that change length when folded
> (`ß`/`SS`, `İ`/`i̇`) cannot match. Other non-ASCII characters compare exactly.

### Reserved words

```nh
reserved from IDENT { "let" "if" "else" "while" }
```

This guards in both directions:

- Keyword literals get a trailing boundary check, so `let` does not match inside
  `letter`.
- `IDENT` rejects reserved words, so `let` cannot be parsed as a variable name.

Write this instead of hand-rolling the lookaheads. They are easy to get subtly
wrong. NailHammer's own meta-grammar was silently broken by exactly that mistake
during development.

**When the boundary cannot be worked out.** The guard is `"let" ~ !<boundary>`,
and the boundary comes from the token's *repeated* part. That works for
`IDENT = ALPHA (ALPHA | DIGIT)*`. A token with no repeated tail cannot be read
that way, and `nh check` tells you rather than guessing:

```
warning: cannot derive an identifier boundary for `TWO` precisely;
         approximating from every character it can match
help: state it outright: `boundary TWO = <what may follow>;`
```

```nh
boundary TWO = ALPHA;
```

An explicit `boundary` wins over the derived one.

### Contextual keywords

Sometimes you want the boundary check but *not* the reservation — a word that
introduces a construct in one place and is an ordinary name in another:

```nh
guard from IDENT { "atom" "place" "word" }
```

| | `reserved from` | `guard from` |
|---|---|---|
| `let` does not match inside `letter` | yes | yes |
| `let` rejected as an identifier | yes | **no** |

`.nh` itself needs this. `atom` is both a keyword inside a `precedence` block and
a perfectly good rule name, and nearly every grammar here contains
`rule atom = ...`. `examples/selfhost/nh.nh` uses one `guard from` block in place
of 29 hand-written lookaheads.

Word operators such as `word "AND"` are always fully reserved. A word operator
cannot also be a variable name.

### Rules and bindings

```nh
rule stmt
  = "let" name:IDENT "=" value:expr ";" -> let
  | value:expr ";"                      -> eval
  ;
```

**`name:expr` is a binding.** It becomes a parameter of the handler. Handler code
never indexes children by position and never walks the tree.

**`lazy name:rule`** is the same, except the handler gets it unevaluated. See
[Control flow](#control-flow).

**`-> label` names the handler** for that alternative. It does not name an AST
type. Each labelled alternative gets one handler file, named `<rule>_<label>`, so
the label does not need to repeat the rule name: `-> let` on `rule stmt` gives
`stmt_let`.

> **Watch out when a rule grows.** A rule with exactly *one* labelled alternative
> collapses: `rule entry = key:IDENT "=" value:v ";" -> entry;` gives a handler
> called `entry`, not `entry_entry`. Add a second alternative and the handlers
> become `entry_<label>`. The filenames change.

**`-> pass`** is a transparent passthrough. No handler is generated; the
alternative evaluates to whatever its single child does.

**`place`** marks an alternative as assignable. It is only legal after a label,
which is what keeps it distinguishable from a rule reference named `place`.

Alternatives are **ordered choice**. The first match wins, and NailHammer never
reorders what you wrote.

### Silent rules

```nh
silent rule item = import_item | use_item | rule_item;
```

A silent rule matches but produces no node, so its children appear directly in
the parent. This is useful for pure alternations that would otherwise add a
wrapper layer.

**You cannot bind a silent rule.** There is no node for the binding to attach to.
Pest rejects it with a message that points at generated `.pest` and names no line
of your grammar, so `nh check` catches it first:

```
error: `thing` binds `item`, which is a `silent` rule and produces no node to bind to
help: drop `silent` from `item`, or bind what it matches instead
```

If you want named parameters, do not make the rule silent.

### Expression syntax

| Form | Meaning |
|---|---|
| `a b` | sequence |
| `a \| b` | ordered choice — first match wins |
| `a*` `a+` `a?` | repetition |
| `!a` | negative lookahead (matches without consuming) |
| `&a` | positive lookahead |
| `( … )` | grouping |
| `"lit"` | literal |
| `^"lit"` | case-insensitive literal |
| `"a".."z"` | character range |
| `NAME` | reference to a rule or token |
| `ANY` | any single character |

### Imports

```nh
// common_lex.nh — a fragment, with no `grammar` declaration
token DIGIT = @ "0".."9";
token IDENT = @ (ALPHA | "_") (ALNUM | "_")*;
```

```nh
// calc.nh — the entry file
grammar Calc;
import "common_lex.nh";
```

- Paths are relative **to the importing file**, not to the working directory.
- A file with no `grammar` declaration is a fragment. Exactly one `grammar`
  declaration is required across the whole set, in the entry file.
- The merge is **flat**. There is no qualification; tokens, skips, and rules all
  share one namespace.
- **Duplicate definitions are errors**, never last-wins. The message names both
  locations.
- A file reached by two different import paths loads once. Diamonds are fine,
  cycles are an error.

---

## Operators

Operators come from a **table**, not from the shape of your grammar. You write
the table in `.nh`, and you implement the semantics in Rust. This section covers
both halves.

### Writing the table

Take a preset and adjust it:

```nh
use operators::c_style;

precedence override {
    remove "," "->";                          // drop what you don't want
    right  "**" above "*" -> pow;             // add exponentiation
    left   "|>" below "||" lazy(rhs) -> pipe;
}
```

Or write one from scratch, which is what a language unlike C needs:

```nh
use operators::none;

precedence {                    // loosest first
    left    word "OR" | word "XOR";
    left    word "AND";
    prefix  word "NOT";
    left    "=" | "<>" | "<=" | ">=" | "<" | ">" -> compare;
    left    "+" | "-";
    left    "*" | "/" | word "MOD";
    prefix  "-";
    right   "^" -> pow;
    atom    atom;
}
```

- **Fixity** is `left`, `right`, `prefix`, or `postfix`.
- **`word "AND"`** marks an operator that is identifier-shaped. It is
  automatically boundary-guarded and added to the reserved set. Do not restate it
  in `reserved from`.
- **`-> role` binds a semantic role.** A role is what the generated trait method
  is named after. Roles are about meaning, not spelling: C's `&` and BASIC's
  `AND` can both bind `bit_and` and share one implementation. Most operators need
  no `->` at all — a built-in spelling-to-role map covers the common ones.
- **Several operators can share one role.** `-> compare` above produces a single
  trait method that takes a discriminant, instead of six near-identical methods.
- **`lazy(rhs)`** overrides a role's default laziness.
- **`atom NAME`** names the rule the operator driver builds expressions from.

A preset has no special powers. Anything a preset does you can write by hand, and
you can put your own table in a file and `import` it.

> **When `expr` exists.** It is generated if your table declares any operators,
> or if your grammar mentions `expr` anywhere. So `use operators::none;` with an
> empty table and no reference to `expr` produces no `expr` rule and no driver at
> all. A language with no operators should not carry unreachable machinery in
> files it owns. Binding `expr` under an empty table still works, which is how
> you write a grammar that gains operators later without rewriting bindings.

### Implementing the semantics

You never write operator *parsing*. You implement only the operations your
language actually has:

```rust
impl generated::dispatch::Operators for Interp {
    fn add(&mut self, lhs: Value, rhs: Value) -> Result<Value> { /* ... */ }
    fn mul(&mut self, lhs: Value, rhs: Value) -> Result<Value> { /* ... */ }

    // One method covers a whole tier bound to one role.
    fn compare(&mut self, lhs: Value, op: CompareOp, rhs: Value) -> Result<Value> {
        match op { CompareOp::Lt => /* ... */ }
    }
}
```

Every method defaults to an `unsupported` error, so declining an operator costs
nothing. `%` stays unimplemented until you want it, and reports itself honestly
if a program uses it.

### Short-circuiting is written for you

`&&` and `||` are lazy in their right operand, and you do not implement them.

Give `Values` a `truthy` — that is the only part specific to your language —
and `nh_handlers!(Interp)` writes the rest:

```rust
impl generated::dispatch::Values for Interp {
    fn truthy(&self, v: &Value) -> bool {
        match v {
            Value::Bool(b) => *b,
            Value::Num(n) => *n != 0.0,
        }
    }
}
```

That is all. `a && b` now evaluates `b` only when `a` is truthy, `a || b` only
when it is falsy, and `a ?? b` only when `a` is null (add `is_null` if your
language has one — it defaults to "never").

There is nothing to remember because there is nothing to write. `if truthy(lhs)
{ rhs } else { lhs }` is not a decision anybody makes; it is what `&&` *means*
once you have said what truth is.

If your language wants different behaviour, that choice lives in the grammar's
operator table, not in your Rust. A BASIC-style `AND` that is bitwise and strict
binds `bit_and` instead and gets no laziness at all.


### Assignment

Mark an assignable alternative `place`:

```nh
rule primary
  = name:IDENT "[" index:expr "]" -> elem place
  | name:IDENT                    -> var  place
  ;
```

You implement two methods. The rest of the family is generated:

```rust
fn assign(&mut self, place: Place<'_, Value>, value: Value) -> Result<Value>;
fn place_read(&mut self, place: &Place<'_, Value>) -> Result<Value>;
```

`compound_assign` (`+=`, `-=`, and so on) is defaulted in terms of those two plus
the arithmetic role. So adding

```nh
right "+=" | "-=" below "=" -> assign;
```

to your table is the entire cost of the whole compound-assignment family.

**A `Place` holds values, not unevaluated nodes.** For `a[f()] += 1`, the
subscript is evaluated once, when the place is resolved — before the read and
before the write. That is what stops a side effect in a subscript running twice,
and it is a property of the type rather than a rule you have to remember.

An assignment target is never evaluated *as a value* either. Assignment is lazy
in its left operand, so `fresh = 3` creates `fresh` instead of failing to read it.

---

## Running a program

You do not write a parse loop.

```rust
let mut sources = SourceMap::new();
let file = sources.load(&path)?;              // yours
let mut cx = Ctx::new(sources);
let mut interp = Interp::default();

match generated::eval_source(&mut interp, &mut cx, file) {
    Ok(value) => { /* .. */ }
    Err(errors) => for d in &errors {          // yours
        eprint!("{}", d.render(cx.sources()));
    },
}
```

`eval_source` parses, renders a parse error into a sentence, collects the syntax
errors that recovery got past, builds the owned tree, and evaluates it — in the
one order that is correct.

**Where the source comes from is yours**, because a file, a socket, and a string
literal in a test are all legitimate. **Where errors go is yours**, because that
is a property of your program: a binary prints them, a test asserts on them, an
editor turns them into squiggles. `eval_source` returns them so all three can
use the same list.

Everything between those two is the same in every project, so it is not yours to
write. `nh init` scaffolds both ends for you.

> **`Ok` means the program was clean.** A parse that *recovered* still gives
> you `Err`, holding the syntax errors, even though everything evaluable was
> evaluated — a reported typo is not a successful run. Anything your handlers
> collected is still there on your host, so a partial run can still show its
> output. The scaffold prints it either way.

---

## Starting a project

```console
$ nh init mylang
```

Run in a terminal it asks two questions; run in a script it takes the defaults.
Either way the same flags work:

```console
$ nh init mylang --style basic --with loops,functions --compiler
```

**`--style`** is syntax. `c` gives braces and semicolons, `basic` gives a
line-oriented language where a newline ends a statement and `WEND` closes a
loop. It is a genuinely different grammar — newlines are not whitespace, and
assignment is a statement rather than an operator, because `=` already means
equality.

**`--with`** is capability: `loops` (`while`, `for`, `do`, with `break` and
`continue`), `functions` (definitions, calls, parameters, `return`, recursion),
or `all` / `none`.

**`--compiler`** picks the other shape — see below.

### The styles share their handlers

This is worth knowing before you pick, because it means the choice is cheaper
than it looks. `WHILE cond ... WEND` and `while cond { }` bind the same names to
the same shapes, so both scaffold the *same* `handlers/stmt_while.rs`:

```rust
pub fn run(host: &mut Interp, cond: &Rc<Expr>, body: &Rc<Block>, cx: &mut Ctx)
```

Change your mind about syntax later and you rewrite the grammar, not the
handlers. The only file the line-oriented style adds is `line.rs`, because a
newline needs a rule to hang on where a `;` does not.

### Language decisions live on the host, not in a handler

The scaffold makes one such decision for you, and shows you where to change it.
Reading a name that was never declared is an **error** in the braced style and
**zero** in the line-oriented one, because that is what BASIC has always done.

The handler does not decide:

```rust
// handlers/primary_var.rs — the same file in both styles
pub fn run(host: &mut Interp, name: &Name, cx: &mut Ctx) -> Result<Value> {
    host.read(name.key(), cx)
}
```

`read` is on your host in `src/lib.rs`, next to the symbol table, and it is
four lines. That placement is the point: a question about what your *language*
means should have one answer in one place, not one per handler that happens to
look a name up.

It also has to be the same answer in both **shapes**. A compiled program and an
interpreted one disagreeing about whether `x` is an error is the one failure
this design exists to prevent, so the scaffold's bytecode VM makes the matching
choice — and `the_two_shapes_agree_about_an_undeclared_name` fails if that ever
drifts.

### One thing that does differ, and why

`--style basic` folds identifier case, as BASIC always has: `Total` and `total`
are one variable. `--style c` does not.

That is a difference in the *language*, not the syntax, and it shows up in the
seven handlers that touch a name. A folding token binds as `&Name` instead of
`&str`:

```rust
// --style c
pub fn run(host: &mut Interp, name: &str,  value: Value, cx: &mut Ctx)
// --style basic
pub fn run(host: &mut Interp, name: &Name, value: Value, cx: &mut Ctx)
```

`Name` keeps **both** spellings, because folding creates two different questions
and neither answer is safe as a default:

```rust
name.key()    // "total"  — the folded form, to look it up
name.text()   // "Total"  — as written, to report it back
```

Return only the folded form and your error says ``undefined variable `total` ``
when they typed `Total`, which reads as a bug in your language. Return only the
raw text and `Total` and `total` become different symbol-table keys, so folding
silently does nothing. The type makes you choose, once, per use.

The scaffolded handlers already choose correctly — `.key()` to look up, and
plain `{name}` in diagnostics, which formats as the text. Turning folding on or
off in your grammar later is one word, and every handler that has to change
stops compiling until it does.

---

## Two shapes: interpreter and compiler

The same grammar and the same handler signatures build either. The structural
difference is one line:

```rust
type Out = Value;   // interpreter: what a node evaluated to
type Out = ();      // stack compiler: nothing is returned; results live on the stack
type Out = Reg;     // register compiler: which register holds the result
```

`nh init --compiler` scaffolds the third, because it is the one worth building
on — see [Registers, and why slots matter](#registers-and-why-slots-matter)
below. `examples/bytecode` is the second, kept because a stack machine is the
shortest way to see the idea.

**Eager parameters give a compiler stack order for free.** They are evaluated
left to right *before* the handler runs, and for a compiler "evaluated" means
"emitted" — so operand code lands before the operator's instruction:

```
2 + 3 * 4     ->    Push 2 · Push 3 · Push 4 · Mul · Add
```

Precedence ends up in the instruction *order*, and `add` is one line:
`self.emit(Op::Add)`.

**`lazy` reads differently in each, and works identically.**

| | interpreter | compiler |
|---|---|---|
| eager binding | already evaluated | already **emitted** |
| `lazy` binding | run it **when** I say | emit it **where** I say |

Without `lazy` a body would already be emitted before the handler could put a
jump in front of it:

```rust
pub fn run(host: &mut Interp, _cond: (), body: &Rc<Stmt>, cx: &mut Ctx) -> Result<()> {
    let jump = host.emit_jump_if_false();   // cond's code is already emitted
    body.eval(host, cx)?;                   // emits the body here
    host.patch_to_here(jump);               // now its length is known
    Ok(())
}
```

Note the inversion: a compiler calls `.eval()` **once**, to emit a body that will
run many times. An interpreter calls it once per execution.

**What differs.** A compiler does not implement `Values` — there is nothing to
inspect at build time — so it opts out and writes its own `ShortCircuit`, which
emits the test rather than performing it:

```rust
nh_handlers!(Compiler, without short_circuit);
```

```
a && b     →     <a> · Dup · JumpIfFalse end · Pop · <b> · end:
```
Non-local control flow differs too: an interpreter unwinds with
`Error::Signal`, while a compiler emits a jump and records its index for
patching, which is host state rather than a signal.

A third shape falls out of the same model: `type Out = Type` with handlers that
check rather than compute is a typechecker.

### Registers, and why slots matter

`type Out = Reg` makes the operator trait read as three-address code, with no
change to the toolkit:

```rust
fn add(&mut self, a: Reg, b: Reg) -> Result<Reg> {
    let dst = self.reuse(&[a, b]);      // frees the operands, takes a destination
    self.emit(Op::Add { dst, a, b });
    Ok(dst)
}
```

Registers are allocated in **stack discipline** — `free` only releases the top
one — which is what keeps an expression's temporaries contiguous. That is not
tidiness: a call needs its arguments in consecutive registers, and eager
parameters evaluated left to right into such an allocator put them there with
nobody arranging it.

**Registers alone buy nothing.** Measured on the scaffold: with variables in a
name-keyed map and registers used only for temporaries, the same program came to
100 instructions either way. The textbook "four dispatches versus one" assumes
the operands are already in registers; when every variable access is a hash
lookup, both shapes pay it.

What pays is a **compile-time symbol table**. Parameters take slots `0..n`,
locals the next free slots, and globals stay named:

| | instructions | name lookups |
|---|---|---|
| stack | 33 | 11 |
| register + slots | 18 | **0** |

Reading a local then emits *no instruction at all* — `primary_var` hands back the
slot. Two things follow, and both are in the scaffold:

* **`free` must skip locals.** A slot belongs to its variable for the whole
  function. Releasing one reuses a live variable, which produces wrong answers
  rather than a compile error.
* **Policy belongs on the host, not in a handler.** `stmt_for` never asks whether
  its counter is a slot or a global; `read_var` and `emit_increment` answer that,
  and they live in your `lib.rs`.

## Writing handlers

```console
$ nh build mylang.nh -o src/mylang.pest --rust src
ok: generated 14 file(s) in src  [9 new handler(s), 0 kept]
```

A scaffolded project depends on **pest and nothing else**:

```toml
[dependencies]
nh-runtime = { path = "vendor/nh-runtime" }
pest = "2.8"
pest_derive = { version = "2.8", features = ["grammar-extras"] }
```

`nh init` vendors the runtime into `vendor/nh-runtime/`, so the project builds
with no credentials, no cargo configuration, and no access to the NailHammer
repository. The copy is pinned to the `nh` that generated it, which is the right
coupling: generated code and its runtime have to agree.

That produces two kinds of file, and the difference matters:

| | |
|---|---|
| `src/generated/**` | **Always overwritten.** The AST, its builder, the trait stack, the evaluator |
| `src/handlers/mod.rs` | **Always overwritten.** It only lists the modules |
| `src/handlers/<alt>.rs` | **Written once.** Yours from then on — never overwritten, never deleted |

### Will a rebuild eat my code?

No. A handler file you have written is never overwritten and never deleted,
however the grammar changes. Rebuild as often as you like; add bindings, rename
rules, delete alternatives — your handler bodies survive all of it.

Only two things ever remove a file, and both are explicit:

| | |
|---|---|
| `--prune` | Deletes orphaned handlers that were **never implemented** |
| `--prune --force` | Also deletes orphaned handlers that contain your code |

"Never implemented" is not a guess. The stub ships with a `compile_error!` whose
message tells you to delete that line, so a file that still contains it cannot
have been finished — it would not compile. Everything else is treated as yours.

Without `--prune`, an orphan is reported and left alone:

```console
warning: 1 handler file(s) no longer match any grammar alternative:
  handlers/entry.rs  (implemented — contains your code)
note: pass --prune --force to remove implemented ones too, but read them first
```

> **The one file to be careful with is `handlers/mod.rs`.** It is regenerated on
> every build, because it is just the list of handler modules. Anything you add
> to it is lost. If you want shared helpers, put them in their own module
> declared from `lib.rs`, not from `handlers/mod.rs`.

Everything under `src/generated/` is regenerated too, and carries a `DO NOT
EDIT` header saying so.

### Wiring it up

```rust
pub struct Interp;

impl generated::dispatch::Semantics for Interp {
    type Out = Value;
    fn truthy(&self, v: &Value) -> bool { !matches!(v, Value::Bool(false)) }
}
impl generated::dispatch::Operators for Interp {}

crate::nh_handlers!(Interp);   // writes the delegating Handlers impl
```

`nh_handlers!` expands to one method per grammar alternative, each delegating to
`handlers::<name>::run`.

Running a program is two steps:

```rust
let pair = MyParser::parse(Rule::program, &text)?.next().unwrap();

let tree  = generated::ast::build_program(pair, file)?;   // parse tree -> owned AST
let value = generated::dispatch::eval_program(&mut interp, &tree, &mut cx)?;
```

The first step is worth having on its own. `tree` owns everything, outlives the
parse, and can be kept, inspected, or run more than once.

**Add an alternative to the grammar and the build fails until you write its
handler:**

```
error: handler `stmt_show` is not implemented. Delete this line, then return
       a value built from the parameters above.
  --> src/handlers/stmt_show.rs:18:5
```

The stub contains a `compile_error!`, not a runtime `todo!`, so an unhandled
alternative cannot ship. Delete that line and write the handler.

### Inside a handler

**A handler's parameters are its bindings.** There is nothing to fetch and
nothing to walk:

```rust
// handlers/entry.rs — from `rule entry = key:IDENT "=" value:value ";" -> entry;`
pub fn run(host: &mut Interp, key: &str, value: Value, cx: &mut Ctx) -> Result<Value> {
    Ok(Value::Field(key.to_string(), Box::new(value)))
}
```

The generated evaluator walked the tree and evaluated the children before calling
you. Here is what each binding turns into:

| Grammar | Parameter | What you get |
|---|---|---|
| `key:IDENT` | `&str` | The token's text |
| `key:IDENT`, folding token | `&Name` | Text plus `.key()` |
| `value:expr` | `Self::Out` | The sub-rule, **already evaluated** |
| `lazy body:block` | `&Rc<Block>` | The sub-rule, **not** evaluated |
| `x:y?` | `Option<..>` | |
| `x:y*` | `&[..]` or `Vec<Self::Out>` | |

Parameters appear in grammar order, and each one is documented on its own line
above the stub. The signature alone tells you what the handler is working with.

`.key()` exists **only** on folding tokens. Calling it in a case-sensitive
grammar is a compile error rather than a silent no-op, so a symbol-table lookup
cannot forget to fold. Use `.text()` in messages — reporting `counter` when
somebody typed `COUNTER` reads like a compiler bug.

**Changing `*` to `?` is a compile error.** Cardinality is in the type.

If a binding is not enough and you want the raw source text, use `cx.text()`.

### Changing a grammar you have already written handlers for

Most edits are caught by the compiler. Two are not, and `nh build` checks those
itself:

| You change | What happens |
|---|---|
| Add an alternative | New stub with a `compile_error!`. Build fails until you write it |
| Delete an alternative | The handler is reported as an orphan. `--prune` removes it if it was never implemented |
| Rename a rule or label | Both of the above at once: a new stub, and the old file orphaned |
| Add or remove a binding | Arity changes. The compiler names the new parameter and its type |
| Change a cardinality or a binding's kind | The type changes. The compiler catches it |
| **Rename a binding** | Nothing the compiler can see. `nh build` **warns** |
| **Reorder two bindings of the same type** | Nothing the compiler can see. `nh build` **errors** |

The last two need explaining, because they are the ones that can bite.

Parameters are matched **positionally**. Rust cannot see a parameter's name
across a call, so renaming `value:expr` to `payload:expr` leaves your handler
compiling and working, with a parameter still called `value`. Nothing is wrong
except that the handler no longer says what it reads.

Reordering is worse. If two bindings have the same type — two `IDENT`s, say —
swapping them in the grammar silently swaps what your handler receives:

```console
$ nh build mylang.nh --rust src
error: handlers/entry.rs takes its parameters in a different order than the grammar binds them
  grammar:  alias, key, value
  handler:  key, alias, value
note: parameters are positional, so this handler now receives them swapped
help: reorder the parameters to match the grammar
```

`nh build` exits non-zero on that, so a `build.rs` will fail the build.

> This is the one property the parameter interface gave up. The older,
> view-based handlers looked children up by name (`view.key()`), so a reorder
> was harmless. Parameters are a better interface in every other respect
> (DESIGN.md §5.4), and this check exists to buy the safety back.

If you rewrite a handler's signature into a shape the check cannot read, it says
nothing rather than guessing. A false alarm on your own code would be worse than
the drift it is looking for.

### Async work in a handler

Handlers are synchronous, because the evaluator is. Making it async would mean
every `eval_*` returned a boxed future — a heap allocation per node — whether or
not a language ever awaits anything.

So async work is **blocked on** rather than awaited. `nh init --async` sets that
up: tokio with the right features, a multi-thread `#[tokio::main]`, and a helper
on your interpreter.

```rust
pub fn run(host: &mut Interp, value: Value, cx: &mut Ctx) -> Result<Value> {
    let body = host.block_on(fetch(url))?;
    Ok(Value::Str(body))
}
```

The obvious spelling of that does not work:

```rust
Handle::current().block_on(fut)
// panics: Cannot start a runtime from within a runtime
```

The thread is already driving the executor. The generated helper uses
`block_in_place`, which hands that thread's other work to a sibling worker
first:

```rust
pub fn block_on<F: std::future::Future>(&self, fut: F) -> F::Output {
    tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(fut))
}
```

That is why `--async` pins `rt-multi-thread` and the multi-thread flavour:
`block_in_place` panics on the current-thread runtime.

**What this costs.** A tokio worker thread is held for the duration of the call.
That is the right trade for a handler that occasionally reaches the network. It
is the wrong one if your *language* has async semantics of its own — an `await`
keyword whose interpreter must yield to a scheduler and interleave with other
tasks. That needs an async evaluator, which does not exist today.

Without `--async` nothing changes: a plain scaffold has no tokio dependency at
all.

### Errors locate themselves

```rust
Err(_) => cx.err("not a valid number"),
```

```
error: not a valid number
  --> config.conf:4:11
   |
 4 |   depth = 9x;
   |           ^^
```

The evaluator enters each node's span before calling its handler, so no handler
threads spans by hand and a new error site cannot forget to.

`cx.err(..)` returns a `Result`. `cx.error(..)` returns the `Error` itself, for
places where a `Result` is the wrong shape.

### Repetitions skip what already failed

In a `Vec` parameter, an item that failed **and was already reported** — a node
the parser recovered from — is left out instead of failing the whole list. That
is what makes `recover` pay off: every statement that can run, runs, and each
reports its own problems. Any other error still propagates.

The run is still a failure. `cx.has_errors()` is true, and the scaffold's
`main.rs` exits non-zero.

---

## Control flow

Handler parameters arrive already evaluated. That is what makes ordinary handlers
two or three lines. It is also wrong for a specific family of constructs: the
ones that decide **whether**, **when**, or **how often** their body runs.

This section covers the four tools for those, roughly in order of how often you
will need them.

### `lazy` — get the node instead of the value

Mark a binding `lazy` and the handler receives the AST node rather than the
result of evaluating it:

```nh
rule stmt = "if" cond:expr "then" lazy body:stmt -> iff;
```

```rust
pub fn run(host: &mut Interp, cond: Value, body: &Rc<Stmt>, cx: &mut Ctx) -> Result<Value> {
    if !host.truthy(&cond) {
        return Ok(cond);
    }
    body.eval(host, cx)                 // this is what runs it
}
```

Without `lazy`, `if false then trace(1)` would still call `trace`.

Use `Semantics::truthy` rather than testing for your own false value. It is your
language's single definition of truth, and the short-circuit defaults for `&&`
and `||` already use it. Hand-writing the test here is how `if 0 then ..` and
`0 && ..` end up disagreeing.

`lazy` on a **token** is an error. A token is already just text; there is nothing
to defer.

**It works on repetitions**, which is what a loop needs. `lazy body:line*` gives
`&[Rc<Line>]`, and the handler runs the whole list once per iteration:

```rust
while /* the loop condition */ {
    for line in body {
        line.eval(host, cx)?;
    }
}
```

`examples/basic-interp` writes `FOR` that way. Its test
`an_empty_range_never_runs_the_body` is the proof: `FOR i = 10 TO 1` must produce
no output at all, which is impossible if the body was evaluated before the
handler ran.

> **`lazy` has a second job, and the name only describes the first.**
>
> ```nh
> | "FUNCTION" name:IDENT "(" lazy params:param_list? ")" EOL*
>     lazy body:line*
>   "END" "FUNCTION"                      -> function
> ```
>
> `lazy body:line*` defers **evaluation** — the body runs at each call.
> `lazy params:param_list?` defers nothing at all. Parameter *names* are not
> expressions; there is nothing there to evaluate. It is how a handler asks for
> the node's **structure** instead of its value.
>
> Both are the same mechanism: "hand me the node, not the result." Only one of
> them is about laziness. `node` or `raw` would name the pair better, and the
> mismatch is worth knowing about when you read a grammar that uses the second
> form.

### Keeping code to run later

A `lazy` parameter is **owned**, so a handler can store it as easily as run it.
That is what makes subroutines and functions possible:

```rust
// handlers/stmt_define.rs — SUB name ... END SUB
pub fn run(host: &mut Interp, name: &Name, body: &[Rc<Line>], cx: &mut Ctx) -> Result<Value> {
    host.subs.insert(name.key().to_string(), body.to_vec());
    Ok(Value::Nothing)
}
```

Cloning a slice of `Rc` copies pointers, not the program. A later `CALL` looks
the body up and runs it:

```rust
for line in &body {
    line.eval(host, cx)?;
}
```

### Signals — `break`, `continue`, `return`, `goto`

A handler returns a value or an error, and for most constructs that is enough. It
is not enough for a jump, because the frame that has to move is somewhere up the
stack.

`?` propagation is already exactly the unwinding a jump needs. The only thing
missing was a variant that is **not a failure**, and that is `Error::Signal`:

```rust
// raising, in the handler for `EXIT FOR`
return Err(cx.signal("EXIT FOR"));
```

```rust
// catching, in the handler that owns the loop
'outer: while /* ... */ {
    for line in body {
        match line.eval(host, cx) {
            Ok(_) => {}
            Err(e) if e.is_signal("EXIT FOR") => break 'outer,
            Err(e) if e.is_signal("CONTINUE FOR") => break,
            Err(e) => return Err(e),
        }
    }
}
```

The runtime never interprets the label. It propagates the signal and, if one
reaches the top uncaught, reports against that name.

Four things to know, all learned by building `examples/basic-interp`:

**Spell the label the way your language does.** It reaches the user:

```
error: `EXIT SUB` is not inside anything that handles it
 --> program.bas:1:1
```

`"exit-sub"` would work identically and leak an internal spelling into a message
about somebody else's code.

**Name the construct, not the action.** Raise `"EXIT FOR"` and `"EXIT WHILE"` as
separate signals rather than one `"break"`. Nesting then resolves itself: an
`EXIT FOR` raised inside a nested `WHILE` is not that loop's signal, so it passes
straight through to the loop that owns it. No depth counting, no bookkeeping, and
no counter to get wrong.

**A handler can also stop a signal.** In that example a `SUB` is a boundary:
`EXIT FOR` inside a subroutine is refused rather than unwinding into whatever
loop happened to call it. That loop encloses the subroutine dynamically but not
lexically, and a jump landing somewhere the source does not show is nobody's idea
of debuggable.

**A value the jump carries rides on the interpreter**, not in the signal. The
runtime has no idea what your values are. `RETURN x` stores `x` and then signals;
the frame that catches it takes the value back out. `GOTO 100` stores its target
line number the same way.

`Error` is `#[non_exhaustive]`, so a `match` on it needs a catch-all arm.

### Driving instead of folding

Most handlers should take their children already evaluated. A construct that
decides *which child runs next* cannot. Mark the list `lazy` and drive it
yourself:

```nh
rule program = SOI EOL* lazy lines:line* EOI -> doc;
rule line    = label:NUMBER? body:stmt EOL*  -> line;
```

```rust
pub fn run(host: &mut Interp, lines: &[Rc<Line>], cx: &mut Ctx) -> Result<Value> {
    let labels = jump_table(lines, cx)?;      // reads `line.label`, runs nothing
    let mut pc = 0;
    while pc < lines.len() {
        match lines[pc].eval(host, cx) {
            Ok(_) => pc += 1,
            Err(e) if e.is_signal("goto") => pc = labels[&host.jump.take().unwrap()],
            Err(e) => return Err(e),
        }
    }
    Ok(Value::Nothing)
}
```

Two properties make this work, and both come from the AST being owned data:

- **You can inspect a node without running it.** `Line` is a typed struct with a
  `label` field, so the jump table is built by reading, not by evaluating.
- **The nodes outlive any single evaluation**, so the driver can go back to one
  it has already passed.

### Functions

Functions put all of the above together, and they need one thing subroutines do
not: **local scope**.

```nh
| "FUNCTION" name:IDENT "(" lazy params:param_list? ")" EOL*
    lazy body:line*
  "END" "FUNCTION"                      -> function
| "RETURN" value:expr                   -> ret
```

```nh
rule primary
  = name:IDENT "(" args:arg_list? ")" -> call_fn    // before `name:IDENT`
  | ...
```

Four things to get right:

- **Put the call alternative before the plain identifier.** Ordered choice takes
  the first match, so `f(1)` must be tried as a call before it is tried as the
  variable `f`.
- **A call is an ordinary operand.** It appears inside expressions and the
  operator driver folds it as an atom, so `f(3) + g(2) * 2` groups the way
  precedence says. Call is a grammar alternative, not an operator.
- **Parameters need a frame.** Push a map of the bound parameters on entry and
  pop it on exit, and look names up in the frame before the globals. Without
  that, a recursive call overwrites its caller's parameters.
- **`RETURN` is a signal carrying a value**, exactly like `GOTO`. Store the value
  on the interpreter, signal, and have the call handler take it back out.

`examples/basic-interp` implements all of this in about sixty lines of handler
code. Its tests cover the parts that fail silently if you get them wrong:
recursion with per-call frames, a parameter not leaking into the caller, wrong
argument counts, and falling off the end without a `RETURN`.

---

## Errors and recovery

### Declaring recovery

```nh
recover stmt sync ";" | "}";
expect "(" in suffix.call as "opening parenthesis of call arguments";
```

`recover` resynchronises at the named tokens, so one bad statement does not hide
every later one.

`expect` replaces a mechanical rule-name error with a sentence a user of *your*
language can act on. The target is a rule, or `rule.label` for one alternative,
so the same character can carry different messages in different places:

```nh
expect ")" in suffix.call as "closing parenthesis of call arguments";
expect ")" in group       as "closing parenthesis";
```

### Reporting

With `recover stmt sync ";";` in the grammar, a program with two broken
statements reports **both**:

```console
$ cargo run -- broken.calc
error: could not parse this `stmt`
 --> broken.calc:2:1
  |
2 | let b = @@@ ;
  | ^^^^^^^^^^^^^
help: skipped to the next sync point and carried on, so errors after this one are real

error: could not parse this `stmt`
 --> broken.calc:4:1
  |
4 | this is not valid at all;
  | ^^^^^^^^^^^^^^^^^^^^^^^^^
```

Call `syntax_errors` after parsing and before evaluating:

```rust
let program = MyParser::parse(Rule::program, &text)?.next().unwrap();

for d in generated::syntax_errors(&program, file) {
    eprint!("{}", d.render(&sources));
}
```

Recovery happens **in the grammar**, not in runtime backtracking, so the shape
stays visible in the generated `.pest`:

```pest
stmt          = { nh_ok_stmt | nh_error_stmt }
nh_error_stmt = { (!(";") ~ ANY)+ ~ (";")? }
```

### Three things that surprise people

**Errors do not cascade.** Evaluating an error node returns
`Error::AlreadyReported`, which propagates without producing a second message.
One bad expression yields one diagnostic, not that plus "undefined variable",
"type mismatch", and every other consequence of half-parsed input.

**Once you recover, the parse stops failing.** With recovery on your statement
rule, unparseable input still produces a tree — every failure becomes an error
node instead of a parse error. `syntax_errors` is what reports them.
`render_parse_error` is for grammars *without* recovery.

**A sync point must consume something.**

```nh
recover stmt sync ";"?;   // error: recovery would never fire
```

The generated error node is `(!(sync) ~ ANY)+`. A sync that matches the empty
string makes `!(sync)` always fail, so recovery silently does nothing. `nh check`
reports it rather than leaving you to wonder.

**Recovery does not compose with block-structured rules.** An error rule matches
any text up to its sync point, so a recovering rule used inside a bounded
repetition will swallow the token that ends the block. If `rule block = "{"
body:stmt* "}"` and `stmt` recovers, the error node eats the closing brace and
`stmt*` never terminates. Attach recovery to a rule used in flat lists — see
`examples/basic-interp/basic.nh` for a grammar that deliberately does without it.

---

## Threads, and what is not decided for you

Nothing in the generated code starts a thread, chooses a runtime, or assumes how
many of either you have. Two things *do* touch the question, and both are yours.

### `Shared<T>`: whether a program can cross a thread

Every rule-typed field in the owned AST is a shared pointer, because that is what
makes the recursion finite and lets a `lazy` binding be stored. Which pointer is a
cargo feature:

```toml
nh-runtime = { path = "vendor/nh-runtime", features = ["threadsafe"] }
```

| | |
|---|---|
| default | `Shared<T>` is `Rc<T>` — cheap, one thread |
| `threadsafe` | `Shared<T>` is `Arc<T>` — the tree is `Send + Sync` |

`Rc<T>` is not `Send`, so with the default a program tree **cannot** cross a
thread boundary at all. That rules out parsing on one thread and running on
another, and sharing a stored function body between workers. Turn the feature on
and both work.

**Flipping it changes no signatures.** Generated code and your handlers both say
`Shared<T>`, which is the whole reason it is spelled that way — `Rc` throughout
would have meant rewriting every handler that takes a `lazy` binding, for a
decision made in a manifest.

It is off by default because a single-threaded interpreter should not pay for
atomic refcounts it never needs, and most interpreters are single-threaded. The
other way round would have been a dictate too.

> **What it does not do:** make *your host* thread-safe. `Shared` decides whether
> the program can move or be shared. Whether your interpreter can is about your
> own state, and is yours.

### `AWAIT` in an expression, for a compiled language

If your language has futures of its own, the answer is not an async evaluator. It
is an **opcode and a suspendable machine** — and the compiler scaffold is already
shaped for it.

Three lines of grammar:

```text
prefix word "AWAIT" -> await;
```

Three of host code:

```rust
fn r#await(&mut self, a: Reg) -> Result<Reg> {
    Ok(self.emit_await(a))
}
```

`r#await` because the role is named after a Rust keyword; the generator escapes
rather than mangles. Now `AWAIT` works anywhere an expression does, including
several times in one:

```basic
PRINT (AWAIT a) + (AWAIT b) * 2
```

**The machine never awaits.** It stops, and says what it is waiting for:

```rust
loop {
    match m.resume() {
        Step::Done => break,
        Step::Failed(e) => return Err(e),
        // Whatever *you* call waiting, in whatever runtime you chose.
        Step::Awaiting(handle) => m.resume_with(resolve(handle).await),
    }
}
```

So the same bytecode is driven by a blocking loop with no runtime at all, by a
multi-thread tokio host, and by a **single-threaded** one — and that last is
exactly where "block on the future" panics. Nothing in the generated VM mentions
a runtime, a future or a thread.

The one thing that makes this possible is where the machine keeps its state: in a
struct, not in local variables. A loop holding `pc` and the frames on the Rust
stack cannot be stopped and started, and converting one afterwards is a rewrite.
That is why the scaffold is built that way even for languages that never suspend
— it costs nothing, and it is the only part you cannot add later.

### `--async`: one way to reach a future, not the way

`nh init --async` adds tokio and a `block_on` helper to *your* `lib.rs`. It is a
starting point you own and can delete, and it makes assumptions worth knowing:

* **tokio specifically**, and its **multi-thread** flavour. The helper uses
  `block_in_place`, which panics on a current-thread runtime.
* **Sync-over-async.** The evaluator is synchronous, so a handler blocks on a
  future rather than awaiting it. That costs a worker thread for the duration.

That trade is right for a handler that occasionally reaches the network. It is
wrong if your *language* has async semantics of its own — for that, compile and
suspend, as above. Nothing forces you to take it: skip `--async` and
the generated code neither mentions nor needs a runtime.

The evaluator is synchronous on purpose. Making it async would mean every
`eval_*` returned a boxed future — a heap allocation per node — whether or not a
language ever awaits anything.

---

## Seeing where a program goes

```console
$ nh trace mylang.nh --source 'let a = 1 + 2 * 3;'
$ nh trace mylang.nh --input program.mylang
$ nh trace mylang.nh --source '...' --json
```

Answers "which handler gets this, and what does it receive?" **without
generating or compiling anything.** `pest_vm` interprets your lowered grammar, so
it costs a parse. Without it, the question is a generate-compile-add-print-
statements round trip to learn something the grammar already decided.

```
stmt_bind  → handlers/stmt_bind.rs
  · "let" name:IDENT "=" value:expr ";" -> bind
  name: &str = "a"
  value: Self::Out   ⟵ evaluated first, by:
    Operators::add
      · `+` — left-associative, precedence 4
      lhs: Self::Out   ⟵ evaluated first, by:
        primary_num  → handlers/primary_num.rs
          · digits:NUMBER -> num
          digits: &str = "1"
      rhs: Self::Out   ⟵ evaluated first, by:
        Operators::mul
          · `*` — left-associative, precedence 5
```

Four things it makes explicit:

* **Which handler**, and the file to open.
* **What it receives** — names, types, and a token's actual text.
* **Which arguments have not been evaluated yet.** A `lazy` one arrives as the
  node; everything else arrives as a value. That is the distinction people get
  wrong.
* **How operators fold.** They route to `Operators::<role>` and to no handler at
  all, nested the way the driver nests them. Nothing else can show you this —
  precedence lives in the operator table, so the parse tree is flat and has no
  order in it.

Two more things it tells you honestly:

* A rule with no `-> label` generates no handler, and says so rather than naming
  a file that does not exist.
* A statement `recover` got past routes **nowhere**, and is shown as such — it
  would otherwise be simply absent, which reads as "handled".

`--rule` starts somewhere other than the first rule declared. The VS Code
extension puts this in a live pane beside your grammar.

---

## Checking your grammar

A PEG will happily accept a grammar that means something other than it looks
like. `nh check` reports the cases where that is **certain**:

```console
$ nh check mylang.nh
warning: this alternative is unreachable: an earlier one matches `let`,
         which is a prefix of `letter`
 --> mylang.nh:8:28
  |
8 | rule kw = "let" -> short | "letter" -> long;
  |                            ^^^^^^^^^^^^^^^^
note: lint: `shadow`
help: ordered choice takes the first match, so put the longer alternative first
```

```console
$ nh check --lints
  left-recursion           a rule that can reach itself without consuming input
  nullable-repetition      a repetition whose body can match nothing
  shadow                   an earlier alternative that makes a later one unreachable
  unreachable-alternative  an alternative after one that always matches
  duplicate-binding        the same binding name twice in one sequence
  unused                   a rule or token nothing refers to
  recover-sync             a `recover` sync point that can match nothing
  silent-binding           a binding onto a rule that produces no node
```

Five of these are **errors**, not warnings, because each one means the grammar
cannot work: `left-recursion`, `nullable-repetition`,
`unreachable-alternative`, `silent-binding`, and `recover-sync`. The other three
are warnings.

### The lints are deliberately quiet

A lint that fires on working code is one you learn to ignore. Every grammar in
this repository analyses completely clean, and that is enforced by a test.

If an analysis cannot be sure, it says nothing. That is a deliberate trade: you
will not get a warning for every questionable construct, but the ones you do get
are worth reading.

### Silencing one

```nh
allow unused in file;
```

Scoped to a single rule, so it cannot quietly disable a whole class of check
across your grammar. Use `--deny-warnings` in CI to make warnings fail the build.

---

## Reading the generated `.pest`

You never edit it, but it is worth knowing how your grammar maps onto it.

```pest
// from:  rule stmt = "let" name:IDENT "=" value:expr ";" -> let | value:expr ";" -> eval;
stmt      = { stmt_let | stmt_eval }
stmt_let  = { nh_kw_let ~ #name = IDENT ~ "=" ~ #value = expr ~ ";" }
stmt_eval = { #value = expr ~ ";" }
```

- **Each labelled alternative becomes its own rule**, named `<rule>_<label>`.
  That rule identifies the alternative in the parse tree, and it is also the
  handler filename.
- **Bindings become node tags**: `name:IDENT` emits `#name = IDENT`.
- `-> pass` alternatives are inlined with no rule of their own.
- Everything NailHammer synthesises is prefixed `nh_`, so it can never collide
  with your names. The one exception is `expr`, which is deliberately yours to
  reference.
- All your `skip` definitions are unioned into pest's `WHITESPACE`.

### Names that collide with pest builtins

You may call a token `NUMBER`. It is the obvious name, so NailHammer makes it
work.

Pest reserves `NUMBER` as a Unicode property, and it rejects a *tag on any
reference whose name is a builtin* even when you have defined a rule with that
name. So `value:NUMBER` would fail to compile, with a message about built-in
rules that points nowhere near your grammar.

NailHammer emits the rule as `NUMBER_` and rewrites every reference. You will see
the suffixed name in the parse tree; nothing else changes. The same applies to
`LETTER`, `ASCII_*`, and the other character-property names.

Structural builtins are different. `ANY`, `SOI`, `EOI`, `NEWLINE`, `PUSH`, `POP`,
`PEEK`, `DROP`, `WHITESPACE`, and `COMMENT` are the vocabulary grammars are
*written in*, so redefining one is an error rather than a rename. A `skip` may
still be called `WHITESPACE`, because skips are renamed regardless.

### Using the `.pest` directly

`nh build` gives you a `.pest`; wire it up the ordinary pest way.

```rust
#[derive(pest_derive::Parser)]
#[grammar = "example.pest"]
struct ExampleParser;

let pairs = ExampleParser::parse(Rule::program, source)?;
```

> Node tags need `pest_derive` built with `features = ["grammar-extras"]`.
> Without it the grammar still compiles, but every tag is silently ignored and
> `as_node_tag()` always returns `None`. `nh init` sets this for you.

### Regenerating automatically

`nh init` writes a `build.rs` that calls the `nh` binary:

```rust
// build.rs
Command::new("nh").args(["build", "mylang.nh", "-o", "src/mylang.pest", "--rust", "src"])
```

Cargo re-runs it whenever the `.nh` file changes. It is safe on every build:
handler files are never overwritten, and output is byte-compared before writing,
so an unchanged grammar does not trigger a rebuild.

It shells out rather than depending on the generator as a crate, which is what
keeps a generated project's dependency list down to pest. Set `NH` if the binary
is not on your `PATH`.

**`nh` is only needed to change the grammar.** The generated code is part of the
project, so somebody who clones it can build and run with nothing installed. If
the `.nh` has been edited and `nh` is missing, the build stops and says so —
rather than quietly compiling the previous grammar.

### Removing an alternative

Deleting an alternative from the grammar leaves its handler file behind.
`nh build` reports it rather than deleting your work:

```console
$ nh build mylang.nh --rust src
warning: 2 handler file(s) no longer match any grammar alternative:
  handlers/stmt_old.rs  (implemented — contains your code)
  handlers/stmt_new.rs  (never implemented)
note: pass --prune to remove the unimplemented ones
```

`--prune` removes only handlers **nobody ever wrote**. The stub's
`compile_error!` says to delete that line, so its absence means somebody did.
Removing a handler that contains code needs `--force` as well, because "this rule
no longer exists" is not the same claim as "you do not want this code".

---

## Known gaps

| | |
|---|---|
| Bounded repetition `{n,m}` | Not implemented. Use longhand |
| Recovery inside block-structured rules | Swallows the closing token; see above |
| The `c_strict` bitwise/comparison lint | Deferred — it inspects a target *program*, not a grammar |

Everything else described in this guide is implemented and tested.
