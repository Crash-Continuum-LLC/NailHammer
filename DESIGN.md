# NailHammer — Design v0.4

A grammar toolkit that compiles a PEG-native intermediate language (`.nh`) into a
Pest grammar plus Rust handler scaffolding, so interpreters and bytecode
compilers can be written across many small files instead of one giant one — with
operator handling supplied as a batteries-included default rather than hand-rolled
per project.

Status: **planning**. Nothing is implemented.

Changes from v0: precedence lowering reversed from stratified ladder to a
generated Pratt driver (§5.2); a standard operator prelude and `Operators` trait
added (§6); spans upgraded from single-file to a multi-file `SourceMap` (§7).

Changes from v0.2: operator tables are fully author-writable rather than
preset-plus-deltas, presets lose privileged status, trait methods bind to
semantic *roles* instead of spellings, and word operators (`AND`, `MOD`) become
first-class (§6). BASIC is the reference stress case.

Changes from v0.3: **all open questions are closed.** Postfix access leaves the
operator table for the grammar (§6.7), `Place` becomes a generated enum with
pre-evaluated payloads (§6.8), error nodes poison via `AlreadyReported` (§5.5),
`.nh` files import each other (§3.1), and operator literals auto-sort by
descending length (§5.2).

---

## 0. The standing principle

**Do not bill the tool writer for a decision that always goes the same way.**

Everything in this document is downstream of one bet: writing a language should
cost you the parts that are *yours* and nothing else. So when a choice always
skews one way, the generator makes it. What the user writes should be the part
only they could have written.

Three tests, applied to every generated API:

1. **Is it a decision, or a consequence?** `if truthy(lhs) { rhs } else { lhs }`
   is not a decision — it is what `&&` *means* for a host with values. The
   decision is `truthy`. Ask for the decision; write the consequence. (§6.9)
2. **If they get it wrong, when do they find out?** Compile time, or it does not
   ship. A default that is silently wrong is worse than no default, and a
   default that is always right is better than either. (§6.9, §5.5)
3. **Does the exception have to pay?** If one host in ten needs something else,
   nine should not carry the ceremony. Make the common case free and let the
   exception say one thing.

This is why handler parameters are bindings rather than a `Pair` to walk, why
`nh_handlers!` writes the dispatch, why the operator roles are defaulted, why
`Place` payloads arrive pre-evaluated, and why `nh init` vendors the runtime.
Each was once a thing a user would have had to write the same way every time.

Where it has been violated, the tell has been the same: a paragraph of
documentation explaining what to type. If the docs have to teach a ritual, the
generator should have performed it. (§6.9 is a worked example of catching this
one commit too late.)

---

## 1. Decisions locked

| Question | Decision |
|---|---|
| Scope | General-purpose toolkit from day one; not tied to one target language |
| `.nh` syntax | PEG-native, purpose-built. No ANTLR `.g4` compatibility, no Pest superset |
| Handler shape | Untyped Pest `Pair`s + registry — **but** with generated per-rule named accessor views |
| Pipeline depth | Parse + dispatch only. No user-facing AST, no bytecode/VM scaffolding |
| Handler return type | Associated `Out` type, shared across the trait stack (§4.1) |
| Multiple passes | Multiple `impl Handlers for PassN`; ordering is plain Rust. No generated pass driver |
| Precedence | **Generated Pratt driver.** Stratified ladder dropped (§5.2) |
| Operator table | Presets have **no privileged status**. `use operators::none` + a bare `precedence { }` writes a table from scratch (§6.1) |
| Operator prelude | `c_style` (default), `c_strict`, `core`, `none` (§6.1) |
| C's `&` vs `==` wart | **Fixed Go-style** in `c_style`. `c_strict` preserves it, with a lint (§6.1) |
| Trait binding | Every operator binds a semantic **role**. Method names never derive from spelling (§6.3) |
| Operator semantics | `Operators` trait, one defaulted method per role; override only what applies (§6.4) |
| Word operators | `word "AND"` auto-guards the identifier boundary **and** auto-reserves (§6.5) |
| Lazy operands | Role sets default laziness; explicit `lazy(..)` overrides. Driver passes `Thunk` / `Place` (§6.6) |
| Postfix access | Call / index / member leave the operator table; grammar suffix chain with ordinary handlers (§6.7) |
| `Place` | Generated enum, payloads **pre-evaluated** so `a[f()] += 1` calls `f()` once (§6.8) |
| Operator ordering | Synthesized alternations auto-sort by descending literal length. User alternations never reordered (§5.2) |
| Error poisoning | Error nodes yield `AlreadyReported`; poisoned subtrees unevaluated, cascades suppressed (§5.5) |
| Imports | `import "x.nh"` for tokens, rules, and precedence blocks. Duplicates are hard errors (§3.1) |
| Keywords | Declared `reserved from` set, auto-guarded in both directions (§5.3) |
| Case folding | Two independent knobs (keywords, per-token). **ASCII-only**, by pest's constraint. Views expose `.text()` + `.key()` (§5.3) |
| Spans | Full multi-file `SourceMap` with `FileId` + global offsets (§7) |
| Errors | Labeled expectation messages **and** grammar-level sync-point recovery (§5.5) |

The four "order and determinism" problems in scope: ordered-choice shadowing,
precedence/left recursion, handler dispatch order, error reporting and recovery.
§5 maps each to its mechanism.

---

## 2. Verified technical foundation

Checked against the toolchain installed here (`rustc 1.95.0`, `pest 2.8.7`
vendored in the local cargo registry), not assumed.

**Pest has node tags.** `pest_meta`'s own grammar defines
`tag_id = @{ "#" ~ ("_" | alpha) ~ ("_" | alpha_num)* }` at term level
(`term = { node_tag? ~ prefix_operator* ~ node ~ postfix_operator* }`), and the
runtime exposes `Pair::as_node_tag()`, `Pairs::find_first_tagged()`,
`Pairs::find_tagged()`.

This is the enabling fact for the whole design: **named field access without an
AST layer.** A `.nh` binding `lhs:expr` lowers to `#lhs = expr` in the generated
`.pest`, and the generated view reads it by name. Grammar edits surface as Rust
compile errors instead of silent misparses.

**Caveat that changes the accessor implementation.** `Pairs::find_tagged` is
built on `self.flatten()` — it searches the *entire subtree*, not direct
children. On a recursive rule like `expr`, an outer node would happily find an
inner node's `lhs`. Generated accessors must therefore **not** call
`find_first_tagged`. They scan direct children:

```rust
self.pair.clone().into_inner()
    .find(|p| p.as_node_tag() == Some("lhs"))
```

**Node tags require a non-default feature.** `#name = expr` does not exist
unless `pest_derive` is built with `features = ["grammar-extras"]`. Without it
the grammar still *compiles* — the tags are simply not honoured, and every
tagged lookup returns `None` at runtime. Since the entire named-accessor design
rests on tags, this is a hard dependency, recorded in the workspace manifest
with a comment so nobody "tidies" it away.

**`grammar-extras` also gates pest's own tag validation** — so it hides its own
absence twice over. `validate_tag_silent_rules`, which rejects tags on built-in
and silent rules, is compiled out without the feature. A test suite that
validates generated grammars through `pest_meta` *without* it therefore checks
tag correctness with tag checking switched off, and passes grammars that
`pest_derive` rejects. That is precisely what happened at M1: every example
lowered "successfully" and the first scaffolded project failed to compile. Every
`pest_meta`/`pest_vm` dependency in this repo enables the feature for that
reason.

**Pest rejects a tag on any reference whose *name* is a builtin, even a
user-defined one.** `validate_tag_silent_rules` tests the name against pest's
builtin set without checking whether the grammar defines a rule by that name —
so `value:NUMBER` fails even though the grammar defines `NUMBER` itself. Since
`NUMBER` is the obvious name for a number token, NailHammer emits such
definitions under a suffixed name (`NUMBER_`) and rewrites every reference.
Structural builtins (`ANY`, `SOI`, `EOI`, …) are a hard error instead: they are
the vocabulary grammars are written in, so shadowing them takes away the only
way to say "any character".

**A lint that fires on working code is worse than no lint.** *New at M4.* The
`unused` pass flagged `atom` on every grammar using an operator preset, because
the `atom` entry lives in the preset rather than in the grammar text — the rule
looked unreferenced while being the single most important rule in the file. It
was caught by requiring the shipped grammars to analyse clean, not by reading
the code. Any analysis added later needs the same gate: **a corpus of known-good
grammars that must produce zero diagnostics.**

**Precedence direction is a coin-flip you can get wrong silently.** The resolved
table lists tiers loosest-first, `nh explain` numbers them loosest-*highest* for
display, and the precedence-climbing builder treats higher as binding *tighter*.
Reusing the display formula for the driver inverted the whole table and made
`&&` bind tighter than `>`, so `a > 10 && b > 100` parsed as `(a > 10 && b) >
100`. Nothing about that is visible in the grammar or in `nh explain` — only in
a program's answer. Worth stating plainly: **display order and driver precedence
are different numbers and must not share a formula.**

**Role-specific short-circuit conditions cannot be shared either.** `&&` returns
its left operand when that operand is *falsy*; `||` returns it when *truthy*.
One generated default served both at first, giving `||` the semantics of `&&`.

**A node tag on a repetition does not tag every iteration.** Pest's grammar puts
the postfix operator *inside* a tagged term, so `#items = value*` tags the
repetition rather than each match — and the first iteration comes back
**untagged**. A view built on that silently drops the first element of every
list. Found by the worked interpreter losing a config file's first key, its
first list element, and its first nested field, all at once. The emitter now
pushes the tag inward: `(#items = value)*`.

**Silent rules are not atomic, and that breaks hand-written keyword guards.**
Discovered while building M0. A rule written

```pest
kw_grammar = _{ "grammar" ~ !ident_cont }   // WRONG
```

is silent but still *non-atomic*, so pest inserts implicit `WHITESPACE` between
the literal and the lookahead. The guard skips the space, tests `!ident_cont`
against the *following identifier*, matches it, and fails. Every keyword-led
rule stops parsing, with no error pointing anywhere near the cause. The fix is
`@`:

```pest
kw_grammar = @{ "grammar" ~ !ident_cont }   // right
```

This is the single strongest argument in the project for §5.3's `reserved from`.
Hand-written boundary guards are wrong by default, the failure is silent, and no
grammar author should be expected to know this. NailHammer's own meta-grammar
shipped broken until a test caught it; generated guards will not have that
option.

**Pest ships `PrattParser`** (`Op::infix/prefix/postfix`, `Assoc`) — and we still
do not use it. See §5.2: its fold is eager by construction and cannot express
short-circuit operators. NailHammer generates its own driver.

---

## 3. The `.nh` language (sketch)

Illustrative, not final. The point is which constructs must exist.

```nh
grammar Calc;

use operators::c_style;              // full operator set, sane precedence

precedence override {
    remove  "," "->";                // no comma operator, no arrow
    right   "**" above "*";          // exponentiation
    left    "|>" below "||" -> pipe; // custom op, custom trait method
}

skip  WHITESPACE = " " | "\t" | "\r" | "\n";
skip  COMMENT    = "//" (!"\n" ANY)*;

token NUMBER = @ digit+ ("." digit+)?;
token IDENT  = @ (alpha | "_") (alnum | "_")*;

reserved from IDENT { "let" "if" "else" "while" "fn" "return" }

rule atom
  = value:NUMBER                          -> num
  | name:IDENT                            -> var
  | "(" inner:expr ")"                    -> pass
  ;

rule stmt
  = "let" name:IDENT "=" value:expr ";"   -> let_stmt
  | "if" cond:expr body:block             -> if_stmt
  | value:expr ";"                        -> expr_stmt
  ;

recover stmt sync ";" | "}";
expect "(" in atom as "opening parenthesis";
```

Constructs the language must carry:

- **`rule` with labeled alternatives.** `-> let_stmt` names a *handler*, not an
  AST type. One handler and one view per labeled alternative.
- **Field bindings** (`name:IDENT`) → pest tags. Bindings are what make accessors
  possible; unbound elements still parse, they're just unnamed.
- **`use operators::*` and `precedence override`** — the operator system (§6).
- **`token`** (atomic, `@`), **`skip`** (implicit whitespace/comments).
- **`reserved from`** for keyword guarding (§5.3).
- **`recover ... sync ...`** / **`expect ... as ...`** for diagnostics (§5.5).
- **`pass`** — transparent passthrough, no handler generated.

Note there is no `expr` rule. `expr` is *supplied* by the operator prelude; the
grammar only provides `atom`. That inversion is the whole point of §6.

**Bootstrapping.** The `.nh` parser is itself written in Pest, from a
hand-maintained `nh.pest`. Self-hosting is a good later correctness signal but is
not on the critical path and must not gate v1.

### 3.1 Imports

`.nh` files import each other. This is not a convenience feature — it is what
makes §6.1's claim true. Presets are "ordinary tables with no privileged status"
only if a user can write their own preset in a file and reuse it; otherwise
built-ins are privileged by being the sole reusable form.

```nh
import "basic_ops.nh";     // a precedence table you wrote
import "common_lex.nh";    // shared tokens, skips, reserved words
```

Tokens, `skip`, `reserved`, `rule`, and `precedence` blocks are all importable.
The merge is **flat, with duplicates as hard errors** — never last-wins, never
silent override:

```
error: token `IDENT` already defined
  --> prog.nh:6:1
   |
 6 | token IDENT = @ (alpha|"_")(alnum|"_")*;
   | ^^^^^^^^^^^
note: first defined here
  --> common_lex.nh:4:1
```

Flat merge over namespacing is deliberate: rule references in a grammar are
already a flat namespace, and qualified names (`common_lex::IDENT`) would have to
thread through tags, view names, handler module paths, and generated `Rule`
variants. The cost of a duplicate error is that you rename one token; the cost of
namespacing is a qualified identifier in five generated artifacts.

Import cycles are an error. Diamond imports (two files importing a common third)
resolve once — the same file imported twice is not a duplicate definition.

---

## 4. Pipeline and crate layout

```
foo.nh
  │
  ├─ nh-syntax      parse .nh → NhAst            (pest-based, bootstrapped)
  ├─ nh-operators   prelude tables; presets, override resolution, `nh explain`
  ├─ nh-analysis    determinism & lint passes    ← the differentiating layer
  ├─ nh-lower       NhAst → .pest source         (tags, keyword guards, recovery)
  ├─ nh-codegen     → ast.rs, dispatch.rs, place.rs, views.rs, handler stubs
  └─ nh-runtime     Ctx, Name, Place, Error/Signal, SourceMap, diagnostics

nh-cli              `nh init`, `nh check [--json]`, `nh build`, `nh explain`
nh-build            build.rs helper — regenerates on `cargo build`

editors/vscode      the VS Code extension (§10). Shells out to `nh`; no server
```

`nh-operators` is separate because both codegen and `nh explain` consume the
resolved precedence table, and `nh-analysis` must validate it.

Analysis is separate from lowering because `nh check` must run standalone and
fast — for CI, and for the editor, which re-runs it on every keystroke (§10).
That separation paid off exactly as intended: the extension needed no new
analysis path, only a `--json` printer over the diagnostics `check` already
produced.

### 4.1 The trait stack

One associated `Out` flows through the stack, so an interpreter, a bytecode
emitter, and a typechecker are three impls over one grammar.

```rust
pub trait Semantics {
    type Out;                                    // the only thing every host must supply
}

pub trait Values: Semantics {                    // §10 — an interpreter; not a compiler
    fn truthy(&self, v: &Self::Out) -> bool;
    fn is_null(&self, v: &Self::Out) -> bool { false }
}

pub trait Operators: Semantics { /* §6.2 — all defaulted */ }

pub trait Handlers: Operators { /* one method per labeled alternative */ }
```

`Semantics` is deliberately one line. Every method that inspects an `Out` lives
on `Values`, because a compiler's `Out` is not a value — it stands for something
the target machine computes later. The split is recorded in §10; the working
proof is `examples/bytecode`, which implements `Semantics`, `Operators` and
`Handlers` and no `Values` at all.

---

## 5. How each named pain is addressed

### 5.1 Ordered-choice shadowing

In a PEG, `a | ab` silently never matches `ab`, and nothing in the grammar text
makes that visible.

`nh-analysis` runs a **shadowing check**: for each alternation, compute a
first-set / prefix approximation per alternative and report when an earlier one
subsumes a later one. Full subsumption is undecidable, so this is conservative:

- exact-prefix detection on literal-led alternatives (the common case),
- first-token-set overlap warnings for rule-led alternatives,
- `@allow(shadow)` so intentional ordering is explicit rather than a standing
  warning.

Also in this pass: unreachable rules, undefined rule references, and
nullable-repetition detection (`(a?)*`, which loops forever in a PEG).

### 5.2 Precedence and left recursion

Direct left recursion is a **hard error** with a fix-it pointing at the operator
system.

**Reversal from v0.** v0 desugared precedence into a stratified ladder in the
generated `.pest`, chosen so parse shape was visible in the grammar. That virtue
only pays off if you read the operator grammar — and the premise now is that
operator handling is boilerplate nobody should think about. Generating fourteen
tiers of `.pest` for a section no one opens is bloat. Precedence moves to a
generated table plus a driver, and the `.pest` keeps one flat rule:

```pest
expr = { pre_op* ~ atom ~ (bin_op ~ pre_op* ~ atom)* }
```

There is no `post_op` in that rule. Postfix access (call, index, member) left the
operator table entirely — see §6.7 — and lives in the grammar's own `atom` as a
suffix chain, which is what keeps this rule to prefix and infix only.

Inspectability is preserved out-of-band rather than lost:

```
$ nh explain expr
 13  = += -= ...   right   lazy(lhs: place)
 12  ?:            right   lazy(then, else)
 11  ||            left    lazy(rhs)
 10  &&            left    lazy(rhs)
  9  == !=         left
  8  < <= > >=     left
  7  |             left
  ...
  1  ! ~ - +       prefix

  atom: `atom` (suffix chain handled in grammar, not here)
```

**Operator literal ordering.** Synthesized operator alternations are sorted by
**descending literal length**, so `<=` is tried before `<`, `||` before `|`, and
`++`/`+=` before `+`. This also yields C's maximal-munch behavior for free
(`a+++b` → `++ +`). Word operators are disambiguated by their identifier-boundary
guard rather than by length, so `OR` never shadows `XOR`.

This auto-sort applies **only** to alternations NailHammer synthesizes.
User-written rule alternations are never reordered — silently permuting someone's
grammar is precisely the nondeterminism this project exists to remove. Those get
the §5.1 shadowing lint instead.

Determinism is enforced by **static validation of the table** rather than by
grammar shape: duplicate operator, conflicting fixity, an `above`/`below`
reference to an unknown operator, and precedence cycles are all `nh check`
errors.

**Why not pest's `PrattParser`.** Its fold calls
`map_infix(lhs_result, op, rhs_result)` — both operands are already evaluated
when the callback runs. That is eager by construction and cannot express `&&`,
which must not evaluate its right operand when the left is false. Adopting it
would forfeit the laziness decision in §6.3.

**Consequence: a narrow internal tree.** To defer an operand, the driver must
know that operand's *extent* before deciding whether to evaluate it. So the
generated expression driver works in two phases:

1. fold the flat pair stream into `OpTree<'i>` — a small internal node graph of
   borrowed `Pair`s, no values allocated;
2. evaluate the `OpTree`, where an unevaluated child *is* a `Thunk`.

This is a deliberate, bounded exception to "no AST layer": it exists only for
expressions, is entirely internal to `nh-runtime`, and is never visible to
handlers. Flagged explicitly so it isn't mistaken for scope creep later.

### 5.3 Keywords, identifiers, and case folding

`reserved from IDENT { "let" ... }` guards in both directions — literals get a
trailing negative lookahead so `let` doesn't match inside `letter`, and the
identifier token rejects reserved words so `let` can't be parsed as a variable
name:

```pest
kw_let        = @{ "let" ~ !(ASCII_ALPHANUMERIC | "_") }
reserved_word = @{ ("let"|"if"|"else"|"while") ~ !(ASCII_ALPHANUMERIC | "_") }
IDENT         = @{ !reserved_word ~ (alpha|"_") ~ (alnum|"_")* }
```

One declaration site to audit, and `nh-analysis` errors if a literal used in a
rule looks like an identifier but isn't in the reserved set.

#### Case folding — two independent knobs

Keyword folding and identifier folding are separate properties, because real
languages use every combination. One switch cannot express them.

```nh
// BASIC — folds both
keywords case-insensitive;
token IDENT = @ case-insensitive (alpha|"_")(alnum|"_")*;

// SQL-ish — keywords fold, identifiers do not
keywords case-insensitive;
token IDENT = @ (alpha|"_")(alnum|"_")*;

// C — neither. The default; write nothing.
```

`keywords case-insensitive` folds literals, word operators, **and the
`reserved_word` guard** — the guard must fold or `And` slips through as a
variable name in a language where `AND` is an operator.

#### Verified constraint: folding is ASCII-only

Pest's `^"..."` calls `Position::match_insensitive`, which is
`slice.get(0..string.len())` followed by `eq_ignore_ascii_case`. Two consequences,
both load-bearing:

1. Folding is **ASCII-only**. Non-ASCII characters compare exactly.
2. The comparison is **byte-length matched**, so any Unicode case pair whose
   length changes under folding (`ß`/`SS`, `İ`/`i̇`) can never match — not merely
   unsupported, but unrepresentable through `^""`.

Unicode folding is therefore not a flag we could flip; it would require
generating explicit character alternations or a custom matcher. It is deferred,
and `nh-analysis` warns when a case-insensitive literal contains non-ASCII rather
than letting it silently half-work:

```
warning: case-insensitive literal contains non-ASCII
  `über` will not match `ÜBER`
  help: write both spellings as separate alternatives
```

This is an acceptable limit in practice: the languages that fold case — BASIC,
Fortran, Pascal, COBOL, SQL — are ASCII-era by construction.

#### `.text()` and `.key()`

Identifier folding is a symbol-table concern, and pest offers no help. Views on
a case-insensitive token expose both forms:

```rust
let name = v.name();
self.vars.get(name.key());   // "counter" — folded, matches `Dim Counter`
```
```rust
// diagnostics still echo what was actually written
cx.err(format!("undefined variable `{}`", name.text()))?;
```

`.key()` is generated **only** on case-insensitive tokens, so calling it in a
case-sensitive grammar is a compile error rather than a silent no-op — and
forgetting to fold at a lookup site can't silently produce a miss.

### 5.4 Handler dispatch order and file explosion

The part that most directly answers the CLAUDE.md complaint about giant
unmaintainable files.

**A handler's parameters are its bindings.** Rename or reorder a binding and
the signature changes, so handlers fail to compile rather than misbehave.

```rust
// rule entry = key:IDENT "=" value:value ";" -> entry;
pub fn run(host: &mut Interp, key: &str, value: Value, cx: &mut Ctx) -> Result<Value>
```

This is a **revision**. The first implementation passed each handler a *view* —
a typed wrapper with a named accessor per binding — and the handler fetched what
it wanted: `view.key().text()`, `dispatch(host, view.value().into_pair(), cx)?`.
Named accessors did remove positional indexing, which was the stated goal, and
it looked fine until the question was asked plainly: *why is the handler
traversing anything at all?* A generator that hands you a tree and a set of
accessors has moved the tedium, not removed it. Every handler still opened with
the same three lines of fetching, and `into_pair` was a word the reader had to
go and learn.

So the traversal moved into generated dispatch, where it is written once:

| Binding | Parameter |
|---|---|
| `key:IDENT` | `&str` |
| `key:IDENT`, case-folding token | `Ident<'_, Rule>` — keeps `.key()` |
| `value:expr` | `Self::Out`, already evaluated |
| `lazy body:block` | `&Rc<Block>` — owned, so it may be kept |
| `x:y?` / `x:y*` | `Option<..>` / `Vec<..>` |

Views still exist and are still generated — they are how dispatch does the
walking, and they remain the entry point for `expr`, whose shape is a flat
operand stream rather than a set of bindings. What changed is that they are the
**mechanism**, not the **interface**.

Two things this cost, both recorded because they are the interesting part:

- **Eager evaluation is now the default**, which is wrong for conditionals. That
  is what `lazy` is for (§6.6), now reachable from any rule rather than only
  from the operator table.
- **A repetition can no longer skip a failed item by hand.** The scaffold's
  `program` handler used to catch `AlreadyReported` per statement, which is what
  made recovery worth having. Generated extraction now does it: an item that
  failed *and was already reported* is dropped from the `Vec` instead of failing
  the list. Any other error still propagates, and `cx.has_errors()` is still
  true, so nothing is silently forgiven.

**Compile-time exhaustive dispatch via a delegating trait.** The generator emits
a trait with one required method per labeled alternative and an impl whose bodies
only delegate to a per-rule module:

```rust
// generated, always overwritten: handlers/mod.rs
impl Handlers for Interp {
    fn stmt_let(&mut self, name: &str, value: Value, cx: &mut Ctx) -> Result<Value> {
        crate::handlers::stmt_let::run(self, name, value, cx)
    }
    // ...one delegation per labeled alternative
}
```

You write only `handlers/stmt_let.rs` — one small file each. Rust's own
exhaustiveness does the checking: add an alternative to the `.nh` and the build
fails until a handler exists. No runtime registry lookup that can silently miss.

**Regeneration policy** — stated up front because getting it wrong destroys user
work:

| Artifact | Policy |
|---|---|
| `grammar.pest`, `rules.rs`, `views.rs`, `ops.rs`, `handlers/mod.rs`, `diagnostics.rs` | Always regenerated. DO NOT EDIT header. Never hand-edited |
| `handlers/<label>.rs` | Written **once** if absent. Never overwritten, never deleted |

Orphaned handler files (label removed from the grammar) are *reported*, not
deleted. `nh build --prune` removes them explicitly — and distinguishes two
cases, because "this rule no longer exists" is not the same claim as "you do
not want this code":

| | |
|---|---|
| Orphan that is still an untouched stub | `--prune` removes it. No work is lost |
| Orphan containing real code | Reported; needs `--prune --force`. Read it first |

The generated stub's `compile_error!` doubles as the marker: it tells you to
delete that line, so its absence means somebody did. That is what separates a
handler nobody ever wrote from one somebody did.

### 5.5 Errors and recovery

PEG has no native recovery, and Pest's error position is the deepest failure —
often not where the mistake is.

**Better messages** come from the grammar: `expect` annotations and rule names
feed a generated `diagnostics.rs` mapping `Rule` → human label, and Pest's
`Error<Rule>` is post-processed through it.

**Recovery is done in the grammar, not the runtime**, which keeps it
deterministic and inspectable. `recover stmt sync ";" | "}"` lowers to an error
alternative:

```pest
stmt       = { stmt_ok | stmt_error }
stmt_error = { #err = ((!(";" | "}") ~ ANY)+ ~ (";" | "}")) }
```

The parse then succeeds and the driver walks for `*_error` nodes, collecting one
diagnostic each. Multiple errors per run, no runtime backtracking machinery.

Risk `nh-analysis` must catch as hard errors: an error alternative that can match
empty, or one positioned so it shadows the success path.

#### Poisoning: what happens when an error node lands inside an expression

Recovery means a parse tree can contain error nodes *anywhere*, including partway
through an expression the §5.2 driver is about to fold. The fold maps such a node
to `Err(Error::AlreadyReported)`:

```rust
OpTree::Error(span) => return Err(Error::AlreadyReported),
```

It propagates without evaluating the poisoned subtree, and the top level filters
it. One bad expression produces one diagnostic:

```
error: expected `)` to close call arguments
  --> prog.bas:7:22
```

...rather than that plus "undefined variable", "type mismatch", and whatever else
a half-parsed expression would provoke. Cascade suppression is the whole point:
multi-error recovery is only useful if the extra errors are *real* ones.

No sentinel value is required from the target. A poison `Out` would let a
typechecker continue and report several independent errors within one expression
— genuinely better for that use case — but it forces every target, including
interpreters with no sensible "error value", to invent one. If a future
typechecking target wants it, `Semantics::poison()` can be added as an optional
override without disturbing this default.

---

## 6. The operator system

The thesis: operator handling is boilerplate in ~80% of cases. NailHammer should
ship it, and let the grammar author supply only the parts that are genuinely
theirs — which types an operator accepts and what it returns.

### 6.1 Presets are ordinary tables

A preset is a prewritten `precedence { }` block and nothing more. It has no
privileged status, no hidden entries, and no capability unavailable to a
hand-written table. `nh explain --source` prints any preset as `.nh` you could
have typed yourself.

| Preset | Contents |
|---|---|
| `operators::c_style` | Full C operator set, **with `&`/`\|`/`^` moved below comparison** (Go's fix) |
| `operators::c_strict` | Bit-exact C precedence, wart included, for porting real C grammars |
| `operators::core` | Arithmetic, comparison, logical, assignment, call/index/member. No bitwise, no comma, no `++`/`--` |
| `operators::none` | Empty. The starting point for a table with nothing in common with C |

The default is `c_style`. It is deliberately **not** named `c`: a genuine C
grammar ported onto it would silently change meaning, and a misleading name is
worse than a longer one. `c_strict` exists for that case and gets the lint:

```
$ nh check
warning: `&` binds tighter than `==` here
  --> input.calc:12:9
   |
12 |     if flags & MASK == 0 {
   |        ^^^^^^^^^^^^^^^^^ parses as `flags & (MASK == 0)`
   |
help: add parentheses: `(flags & MASK) == 0`
```

A preset can be adjusted in place via `precedence override { remove … }` /
`… above …` / `… below …`, or discarded entirely.

### 6.2 Writing a table from scratch — BASIC as the stress case

BASIC breaks a C-derived table in four independent ways, which is why
delta-only override was rejected: its operators are *words*, `^` is
exponentiation rather than XOR, `NOT` binds **looser** than comparison (so
`NOT A = B` means `NOT (A = B)` — the opposite of C's `!`), and classic
`AND`/`OR` are bitwise and **non**-short-circuiting.

```nh
grammar Basic;
use operators::none;

keywords case-insensitive;

precedence {                    // lowest → highest
    left   word "OR" | word "XOR";
    left   word "AND";
    prefix word "NOT";          // looser than comparison
    left   "=" | "<>" | "<" | "<=" | ">" | ">=" -> compare;
    left   "+" | "-";
    left   "*" | "/" | word "MOD";
    prefix "-";
    right  "^" -> pow;          // exponent, not xor
    atom   primary;
}
```

Note what is *absent*: `=` binds `compare`, not `assign`. BASIC's assignment is a
statement (`LET X = 1`), so `=` in expression position is unambiguously equality
and needs no context-sensitivity mechanism. Assignment is an ordinary
`rule stmt` alternative with an ordinary handler file.

### 6.3 Roles, not spellings

Every operator entry binds a **semantic role**, and the generated trait method
takes its name from the role. Method names never derive from spelling, so
changing an operator's spelling never orphans handler code, and two languages
that spell an operation differently share one implementation:

```nh
left "&"        -> bit_and;    // C
left word "AND" -> bit_and;    // BASIC
```
```rust
fn bit_and(&mut self, l: Out, r: Out) -> Result<Out, Error>;   // both
```

Most entries need no `->`. A built-in **spelling → role** map covers the common
operators (`+`→`add`, `==`→`eq`, `&&`→`and_then`, …), so `left "+" | "-";` binds
correctly with nothing written. An explicit `->` overrides it. A spelling absent
from the map with no `->` is a `nh check` error, not a guess.

Core role vocabulary (the map is documented and stable, not internal):

| Group | Roles |
|---|---|
| Arithmetic | `add` `sub` `mul` `div` `rem` `pow` `neg` `pos` |
| Bitwise | `bit_and` `bit_or` `bit_xor` `bit_not` `shl` `shr` |
| Logical | `and_then` `or_else` `not` `coalesce` |
| Comparison | `eq` `ne` `lt` `le` `gt` `ge` `compare` |
| Mutation | `assign` `compound_assign` `inc` `dec` |
| Other | `ternary` `range` `concat` |

There is deliberately no *Access* group. `call`, `index`, and `field` are not
operator roles — see §6.7.

**Grouped roles.** When several operators in a tier bind one role — as with
`-> compare` above — the generated method receives a discriminant, so one
implementation covers the tier:

```rust
fn compare(&mut self, l: Out, op: CompareOp, r: Out) -> Result<Out, Error>;
```

This is how a language avoids writing six near-identical comparison methods, and
it is the same trade the earlier "one handler for all binary ops" option offered
— now available per tier instead of globally.

A role outside the vocabulary (`-> pipe`) generates a **required** method with no
default, so a custom operator can't be silently forgotten.

### 6.4 The `Operators` trait

One method per role in the resolved table, **every known role defaulted** to an
`Unsupported` error. A language with only arithmetic implements five methods and
gets the whole table's parsing, precedence, associativity, and short-circuit
behavior for free.

```rust
pub trait Operators: Semantics {
    fn add(&mut self, l: Self::Out, r: Self::Out)
        -> Result<Self::Out, Self::Error> { Err(Self::unsupported("+")) }
    // ...one per role, all defaulted
}

impl Operators for Interp {
    fn add(&mut self, l: Value, r: Value) -> Result<Value, Error> {
        match (l, r) { /* your type rules and return types */ }
    }
}   // everything else: automatic
```

Type-directed behavior and return types live inside these bodies — the one place
they genuinely belong.

### 6.5 Word operators

`word "AND"` marks an operator whose spelling is identifier-shaped. The lowerer
does **two** things, because there are two distinct failure modes and only
handling one leaves a silent bug:

```pest
op_and        = @{ ^"AND" ~ !(ASCII_ALPHANUMERIC | "_") }   // ANDY ≠ AND ~ Y
reserved_word = @{ (^"AND" | ^"OR" | ^"NOT" | ^"MOD")
                   ~ !(ASCII_ALPHANUMERIC | "_") }
IDENT         = @{ !reserved_word ~ .. }                    // AND is not a name
```

Word operators are added to the reserved set automatically; they do not need
restating in `reserved from`. `nh-analysis` errors if a `word` operator's
spelling cannot match the grammar's identifier token, since that entry could
never fire.

### 6.6 Lazy operands

Not every operator evaluates both sides. **Each role carries a default
laziness**, so choosing the right role usually settles it with nothing written;
an explicit `lazy(..)` overrides for custom operators.

```nh
left word "AND"     -> bit_and;    // strict, from role
left word "ANDALSO" -> and_then;   // lazy in rhs, from role
left "|>" lazy(rhs) -> pipe;       // explicit, custom role
```

Laziness changes the generated signature — a lazy operand arrives as an
unevaluated node, or as a `Place` (an assignable location), rather than `Out`:

```rust
fn bit_and(&mut self, l: Out, r: Out)      -> Result<Out>;
fn and_then(&mut self, l: Out, r: Rc<Expr>, cx: &mut Ctx) -> Result<Out> {
    if !self.truthy(&l) { return Ok(l) }
    r.eval(self, cx)
}   // ^ default impl — you write nothing

fn assign(&mut self, p: Place<'_, Out>, r: Out) -> Result<Out>;  // no sane default
```

> Written at M3 as `Thunk<Out>` with `r.force()`. The type became `Deferred`,
> then `Rc<Expr>` when the AST became owned (§9). The *shape* — a lazy operand
> is a node the handler chooses whether to run — never changed.

`and_then`, `or_else`, `coalesce`, and `ternary` ship with correct defaults built
on `truthy()`. `assign` is the one role with no meaningful default, since only
the target language knows what a place is.

This is precisely the distinction BASIC needs: `AND` bound to `bit_and` is strict
and bitwise, while the same spelling bound to `and_then` would short-circuit. The
table records the choice; the signature enforces it.

**Laziness is not only an operator concern.** Once handler parameters arrived
pre-evaluated (§5.4), every construct that must *decline* to evaluate its body
needed the same escape — a conditional statement is the obvious one. So `lazy`
became a modifier on any binding, and `Thunk`/`Deferred` unified into one type
that wraps either an unevaluated node or an unevaluated operand subtree:

```nh
rule stmt = "if" cond:expr "then" lazy body:stmt -> iff;
```

```rust
pub fn run(host: &mut Interp, cond: Value, body: &Rc<Stmt>, cx: &mut Ctx)
    -> Result<Value>
{
    if !host.truthy(&cond) { return Ok(cond) }
    body.eval(host, cx)
}
```

The condition goes through `Semantics::truthy`, not a match on the host's own
false value. That is the same predicate the `and_then`/`or_else` defaults use,
and writing it out by hand is how a language acquires two notions of truth —
`calc-interp` counts `0` as falsy, so `if 0 then ..` and `0 && ..` would have
disagreed. `a_conditional_uses_the_languages_own_truthiness` holds the line.

`lazy` on a *token* is an error: a token is already just text, so there is
nothing to defer, and accepting it would suggest it did something.

It composes with cardinality, and that is what makes **loops** expressible:
`lazy body:line*` is a `&[Rc<Line>]` the handler runs once per iteration.
`examples/basic-interp`'s `FOR`/`NEXT` is written that way, and the property
worth testing is the empty range — `FOR i = 10 TO 1` must run its body *zero*
times, which no eager parameter can express.

**Adding the keyword had a cost worth recording.** `.nh` has no reserved words,
so `lazy` was already a legal binding name — and `examples/selfhost/nh.nh` binds
one, in the rule that parses `lazy(rhs)` itself. Naively marking `lazy` as a
keyword broke the self-hosted grammar. The fix is a one-character guard:

```pest
lazy_marker = ${ kw_lazy ~ !":" }
```

`lazy` immediately followed by `:` is a binding name; otherwise it is the
marker. This is the general shape of every keyword decision in a language that
reserves nothing — see §5.3 — and it is why the self-hosting test is worth
running: nothing else would have caught it.

### 6.7 Postfix access is not an operator

Call, index, and member access cannot share a signature: call takes an argument
*list*, index takes an expression, and member access takes a *name* — not a value
at all. Forcing them into one `postfix(lhs, op, operands)` shape would reinstate
exactly the untyped positional destructuring this design exists to remove.

They leave the operator table entirely and live in the grammar as a suffix chain:

```nh
rule atom = primary suffix*;

rule suffix
  = "(" args:expr_list ")"   -> call
  | "[" index:expr "]"       -> index          place
  | "." name:IDENT           -> field          place
  | "?." name:IDENT          -> optional_field
  ;
```

Each becomes an ordinary handler file with named accessors:

```rust
// handlers/call.rs
pub fn run(s: &mut Interp, v: CallView, cx: &mut Ctx) -> Result<Value> {
    let callee = cx.eval(v.callee())?;
    let args: Vec<_> = v.args().map(|a| cx.eval(a)).collect::<Result<_>>()?;
    s.invoke(callee, args)
}
```

Three arguments for this, in ascending order of importance:

1. **Precedence still works with no table involvement.** `atom = primary suffix*`
   binds tighter than any prefix or infix operator by construction, so `-a.b` is
   `-(a.b)` and `f(x)[0].y` chains correctly.
2. **The driver gets simpler** — the fold handles prefix and infix only.
3. **These aren't boilerplate.** The operator system exists to absorb work nobody
   should have to think about. Call semantics (arity, closures, varargs, method
   binding) and field access (property lookup, prototype chains, optional
   chaining) are core language design. They belong in handler files you write,
   not behind a defaulted trait method.

Optional chaining (`?.`) shows the payoff: in the table it would need a lazy
operand and a new role, and as a suffix alternative it is one more line.

### 6.8 Assignment and places

Suffix and primary alternatives marked `place` are assignable. The generator
emits a `Place` enum with exactly those variants:

```rust
pub enum Place<'i> {
    Var   { name: Ident<'i> },
    Index { base: Out, index: Out },      // pre-evaluated
    Field { base: Out, name: Ident<'i> },
}
```

**Payloads are pre-evaluated, and that is the load-bearing detail.** `a[f()] += 1`
must call `f()` exactly once. If `Place` held thunks, the compound-assignment
default would force the index twice — once to read, once to write — and the bug
would be invisible until someone put a side effect in a subscript. Resolving the
place once, up front, makes double evaluation unrepresentable rather than merely
discouraged.

That is what buys a correct default for the entire compound-assignment family:

```rust
fn compound_assign(&mut self, p: Place, op: BinOp, r: Out) -> Result<Out> {
    let cur = self.place_read(&p)?;
    let new = self.apply(op, cur, r)?;
    self.assign(p, new)
}   // ^ default — `+= -= *= /= %= &= |= ^= <<= >>=` all work once
    //   you implement `assign`, `place_read`, and the arithmetic roles
```

`nh-analysis` errors if the table binds `assign` or `compound_assign` but the
grammar marks no `place` alternatives — a table that can parse `x = 1` but has
nowhere to store it is a mistake worth catching at build time.

---

## 7. Spans and the SourceMap

Full multi-file from the start: `SourceMap` owns interned sources keyed by
`FileId`, and `Span { file: FileId, lo: u32, hi: u32}` is global. Pest's
`Span<'i>` carries offsets into a single input; the driver maps input → `FileId`
at parse entry, and views expose the mapped `nh::Span` (with the raw pest span
available when needed).

`Ctx` keeps a span stack, so an error raised anywhere in a handler is tagged with
the innermost node's location automatically — no per-handler threading:

```rust
cx.err("type mismatch: Int + Str")?;
```
```
error: type mismatch: Int + Str
  --> src/input.calc:4:11
   |
 4 |     let x = a + "nope";
   |             ^^^^^^^^^^
```

This is more than the first milestone strictly needs. It is chosen up front
because target languages will want `import`/`include`, and because retrofitting a
`FileId` through every span, diagnostic, and view signature later is far more
expensive than carrying it from the start.

---

## 8. Milestones

1. **M0 — `.nh` meta-grammar. ✅ Done.** Hand-written `nh.pest`; `nh-syntax`
   parses to `Ast`; `nh check` prints it. Includes `import` resolution (§3.1) —
   cycle detection, diamond dedup, duplicate-definition errors — since every
   later milestone consumes a merged `Ast` rather than a single file's. Ships
   with `example.nh`, `examples/calc.nh`, and `examples/basic.nh`; BASIC parses,
   which is the first evidence the `.nh` syntax is not quietly C-shaped.
2. **M1 — Lowering. ✅ Done.** `.nh` → `.pest`: bindings → tags, labelled
   alternatives → sub-rules, reserved-word guarding in both directions,
   case-folding knobs (§5.3), skips unioned into `WHITESPACE`, and the flat
   `expr` rule with length-sorted operator alternations. `nh build` and
   `nh explain` work.

   **Scope adjustment: preset *table data* moved forward from M3.** M1 cannot
   emit the `expr` rule without knowing the operators, and a milestone whose
   flagship examples don't lower is not a milestone. The tables are separable
   from the driver: `nh-operators` now holds the presets, spelling→role map,
   and override resolution, while `OpTree`, `Thunk`, and the `Operators` trait
   remain M3. The presets are stored as **ordinary `.nh` source parsed by the
   real parser**, which makes §6.1's "no privileged status" true in the
   implementation rather than only in this document.

   Verified by parsing real programs, not by inspecting emitted text:
   `pest_vm` interprets the generated grammar at runtime, so the tests run
   `.nh` → `.pest` → parse end to end. All three examples parse, including
   BASIC with word operators and case folding.
3. **M2 — Views, dispatch, SourceMap. ✅ Done.** Direct-child tag accessors,
   the trait stack, the `nh_handlers!` delegation macro, stub generation, spans
   with auto-attach, and `Place`. `nh build --rust` writes them.

   **The thesis holds.** `examples/config/` is a complete interpreter: a
   grammar, nine handler files of a few lines each, and its own tests. There is
   no `into_inner()` and no positional child access anywhere in the
   hand-written code. Cardinality is carried in the accessor type, so changing
   `*` to `?` in the grammar breaks compilation rather than behaviour, and
   `cx.err(..)` locates itself because dispatch wraps every call in
   `cx.scoped(..)`.

   Two design refinements the implementation forced:

   * **A rule with exactly one labelled alternative needs no sub-rule.** `rule
     entry = .. -> entry;` emits `entry`, not `entry_entry`, and its bindings
     are reachable through a view named after the rule.
   * **View built-ins live on a `View` trait, not as inherent methods.** A
     grammar may bind a field named `text` or `span`; because generated
     accessors are inherent and Rust resolves those first, the author's binding
     wins and the built-in stays reachable as `View::text(&view)`. Inherent
     built-ins would have made such a grammar fail to compile for no visible
     reason.
4. **M3 — The operator driver. ✅ Done.** `OpTree` + precedence
   climbing in `nh-runtime`, a generated `op_info` table, `eval_tree`, and
   `Deferred` for unevaluated operands. `Handlers::expr` now has a working
   default, so a language implements `Operators` roles and gets folding free.

   **Short-circuiting is proved by observation, not assertion.**
   `examples/calc-interp` has a `trace(x)` construct that records that it ran;
   `false && trace(1)` leaving no trace is what the test checks. The
   interpreter never implements `and_then` or `or_else` — the generated
   defaults do it, using only the `truthy` the language supplied.

   `Deferred` resolved residual risk #2 more easily than feared: it borrows the
   **tree**, not the host, so `rhs.force(self, cx)` inside a method already
   holding `&mut self` is not a borrow conflict. No index-into-`OpTree` scheme
   was needed.

   **`Place` and assignment landed too, and the acceptance criterion holds:**
   `a[trace(0)] += 1` evaluates the subscript exactly once. That works because
   the place is *resolved* before either half of the compound assignment runs,
   with its expression fields already evaluated — §6.8's "pre-evaluated
   payloads" turning double evaluation from discouraged into unrepresentable.

   The split that made it work: `assign(place, value)` is the primitive a
   language implements, and `compound_assign(place, op, rhs)` is **defaulted**
   in terms of it plus the arithmetic role. `examples/calc-interp` implements
   `assign` and `place_read` and never touches `compound_assign`; adding
   `right "+=" | "-=" below "=" -> assign;` to the grammar was the whole cost
   of the `+=` family.

   An expression binding inside a `place` alternative must have cardinality
   one, or "evaluated once" is meaningless. The generator emits a
   `compile_error!` naming the rule and binding rather than producing something
   that compiles and misbehaves.
5. **M4 — Analysis. ✅ Done.** `nh-analysis` runs six passes:
   `left-recursion` and `nullable-repetition` and `unreachable-alternative` as
   **errors** (each is PEG-fatal), `shadow` and `duplicate-binding` and
   `unused` as warnings. `nh check --deny-warnings` is the CI gate;
   `nh check --lints` lists them; `allow <lint> in <rule>;` silences one.

   **Every lint is conservative, and that is the design.** The shadow check
   fires only when an earlier alternative matches a fixed string that is a
   strict prefix of what a later one must start with — so `"let" | "letter"` is
   reported and `"let" IDENT | "letter" IDENT` is not, because there the PEG
   backtracks and both are reachable. `nullable` treats unknown references as
   non-nullable. Where a check would have to guess, it stays silent.

   The reason is not politeness: **a determinism warning that cries wolf is one
   people learn to ignore**, and then it protects nobody. The test that keeps
   this honest is `the_shipped_grammars_produce_no_diagnostics` — all five
   shipped grammars must analyse completely clean. It failed on first run and
   caught a real false positive (see below).

   *Not done:* the `c_strict` bitwise/comparison lint from §6.1. That one
   inspects a **target program**, not a grammar, so it belongs in generated
   parser code rather than in `nh check`. Deferred rather than faked.
6. **M5 — Diagnostics and recovery. ✅ Done.** `recover R sync X;` lowers to a
   grammar-level error alternative — `R = { nh_ok_R | nh_error_R }` with
   `nh_error_R = { (!(X) ~ ANY)+ ~ (X)? }` — so recovery stays readable in the
   generated `.pest` rather than hiding in runtime machinery. `syntax_errors()`
   walks for those nodes and yields **one diagnostic per failure**, and dispatch
   returns `AlreadyReported` for them so a bad statement produces one message
   rather than every consequence of it.

   `expect "(" in R as "..."` works by promoting the literal to its own
   **silent** rule. Pest reports the *rules* it expected and never bare
   literals, so a literal written inline can never appear in a message; giving
   it a name is what makes the label reachable. Silent matters — a non-silent
   rule produces a pair and changes the tree's shape, which broke transparent
   delegation the first time this was written (`"(" expr ")"` went from one
   child to two).

   **A consequence worth knowing:** once the top-level statement rule recovers,
   the parser essentially stops failing outright. Every failure becomes an error
   node instead. `render_parse_error` therefore matters most for grammars
   *without* recovery; `syntax_errors` is the one to call when you have it.

   The `recover-sync` lint closes the loop: a sync expression that can match
   empty makes `!(sync)` always fail, so the error node is unmatchable and
   recovery silently does nothing. That is now an error rather than a mystery.
7. **M6 (stretch) — Self-hosting. ✅ Done, with the goal corrected.**
   `examples/selfhost/nh.nh` describes `.nh` in `.nh`, and a parser generated
   from it parses **every `.nh` file in the repository, including itself**.

   **The original framing was wrong and is worth correcting rather than
   quietly satisfying.** "`nh.nh` reproduces `nh.pest`" is unreachable:
   `nh.pest` uses silent rules (`_{}`), compound-atomic rules (`${}`), and
   bounded repetition (`{1,6}`), none of which `.nh` can express. None of them
   changes which *strings* are accepted, only the tree shape and the `.pest`
   text — so the achievable and meaningful property is **language
   equivalence**, and that is what the test asserts.

   Two findings came out of writing it, both about `.nh` rather than pest:

   * **`reserved from` conflates two things.** `.nh` reserves nothing globally
     — `atom` is both a precedence keyword and an ordinary rule name — but
     `reserved from` would both guard the literal *and* forbid it as an
     identifier. Only the first is wanted. The first draft reserved the
     keywords and immediately rejected `rule atom = primary;`, a line in nearly
     every grammar here. **A guard-without-reserving form is a missing
     feature**, and all 29 keywords are hand-guarded in the meantime.
   * **The M0 keyword bug reappears one level up.** Written as
     `rule kw_x = "x" !IDENT_TAIL`, the guard silently fails — a rule is
     non-atomic, so pest inserts whitespace and the lookahead tests the wrong
     thing. Exactly the defect from §2, reproduced the moment a user
     hand-writes a guard in `.nh`. It works only as `token KW_X = @ "x" !IDENT_TAIL`.
     Nothing in the language prevents the broken form.

7. **M7 — the owned AST. ✅ Done.** Typed owned nodes generated from the
   grammar, plus a builder that folds operators once, at construction. Handler
   parameters lost both lifetimes; a `lazy` binding became storable. This is
   what made `GOTO`, subroutines, and functions expressible at all — see §9 for
   why it was needed and what it cost.
8. **M8 — editor integration. ✅ Done.** `nh check --json`, a structured lint
   code on every diagnostic, and a VS Code extension: highlighting, live
   diagnostics, context-aware completion, go-to-definition, hover, outline,
   scaffolding, and tasks. See §10.

9. **M9 — the second host shape. ✅ Done.** `Values` split out of `Semantics`,
   so a host whose `Out` is not a value need not answer questions about one;
   short-circuiting written by `nh_handlers!` from the one thing that *is*
   language-specific. `nh init --compiler` scaffolds a register machine with
   slot-allocated locals, and an end-to-end test asserts both shapes print the
   same thing across every style and feature combination. See §10.
10. **M10 — `nh trace`. ✅ Done.** What a program routes to, and with what,
    without generating or compiling anything: `pest_vm` interprets the lowered
    grammar and the node tags are read back out. Arguments own their subtrees,
    `lazy` is marked, and operators are folded by the table's precedence — which
    the parse tree cannot show, being deliberately flat (§5.2). Surfaced as a
    live pane in the extension.

**Every milestone, including the stretch goal, is complete.** What remains is
the residual risks in §11.

### 8.1 `nh init` carries the conventions

Scaffolding is not a convenience feature here. Two of this project's settings
fail **silently** when omitted — an unanchored entry rule works until someone
leaves a blank line at the top of a file, and a missing `grammar-extras`
compiles, parses, and returns `None` from every tag lookup forever. Neither
produces a message pointing at the cause.

Documentation cannot fix a silent failure, because nobody reads closely enough
to prevent a problem they have not met yet. A template can: `nh init` generates
an anchored entry rule, a `Cargo.toml` with the feature enabled, and a sample
program whose leading comment exercises the anchoring. Tests assert all three,
and one ignored end-to-end test compiles the scaffolded project with real
`pest_derive` and checks that tags survive into its output.

The rule this sets for later milestones: **when a convention is load-bearing and
its violation is silent, put it in the template and test the template.** Prose
is the fallback, not the mechanism.

**A scaffold rots faster than documentation does.** Written at M1, it still
generated a parse-tree printer after M2–M5 had landed — so the primary
onboarding path silently misrepresented the toolkit, showing none of views,
handlers, operators, or recovery. Nothing failed; it just described an older
product. It now scaffolds a complete working interpreter, and two `#[ignore]`d
end-to-end tests compile the generated project and assert its actual output
(`28 22 5 14`) and its recovery behaviour. **A milestone that changes what
users write must update the scaffold, and the scaffold's test must assert
behaviour rather than file existence** — otherwise the check passes while the
content goes stale.

### 8.2 `USAGE.md` grows with the milestones

`USAGE.md` is a standing deliverable, not a final one. It is written from the
perspective of someone building a language *with* NailHammer, and each milestone
that lands user-facing capability updates it in the same change:

| Milestone | What `USAGE.md` gains |
|---|---|
| M0 | `.nh` syntax reference; `import` and file layout; running `nh check` |
| M1 | Tokens, `skip`, `reserved`, case folding; reading generated `.pest` |
| M2 | **First end-to-end tutorial** — grammar → handlers → working interpreter. Views, `.text()`/`.key()`, spans, the regeneration policy, `place` |
| M3 | Operator presets, writing a table from scratch, roles, laziness, `nh explain`. C and BASIC as worked examples |
| M4 | The lint catalogue: what each diagnostic means and how to fix or suppress it |
| M5 | `recover`/`expect`, multi-error output, poisoning behavior |
| M7 | Handler parameters, `lazy` and its two jobs, signals, driving vs folding, functions |
| M8 | `--json`, the extension, and how to change a grammar you already have handlers for |

Two rules keep it honest:

- **Nothing undocumented ships.** A milestone isn't done until `USAGE.md`
  covers it. This design document records *why* decisions were made; `USAGE.md`
  records *how to use* what they produced. They are different audiences and must
  not be merged.
- **Claims in the guide are checked where they can be.** Every `.nh` snippet
  added since M7 appears verbatim in a shipped grammar, and a sweep verifies
  that. Extracting and *running* every code block is not done — this document
  said it was, which was an aspiration written as fact, and the guide drifted
  anyway: a duplicated 57-line block, two `## Operators` sections, and a claim
  that renaming a binding is a compile error, which stopped being true at the
  parameter pivot. Prose rots exactly like a scaffold does (§8.1), and for the
  same reason: nothing failed when it did.

M2 is the first milestone where a genuinely useful guide is possible, since it is
the first point an end-to-end interpreter runs. M0 and M1 contribute reference
material that the M2 tutorial builds on.

---

## 9. M7 — the owned AST

The largest change the project has made, and the one with the clearest cause.

### What was impossible, and how it was found

Writing a mini BASIC (`examples/basic-interp`) went well until `GOTO`. Making
the program's line list `lazy` so a handler could drive it produced two compiler
errors, and they were different problems:

```
error[E0599]: no method named `label` found for reference `&Deferred<'_, '_>`
```

A `Deferred` was **opaque**. You could force it or not force it; you could not
ask what it was. A jump table needs each line's label without running the line.

```
error: lifetime may not live long enough
   |     host.saved = lines;
   |     ^^^^^^^^^^ assignment requires that `'1` must outlive `'static`
```

A `Deferred` was **unstorable**. It borrowed the parse tree, and each handler
method took it with fresh anonymous lifetimes independent of `&mut self`, so it
could not outlive the call that received it.

These are the same problem wearing two hats, and it is not really about control
flow: **there was no first-class handle to a piece of unevaluated program.**
`GOTO`, `GOSUB`, `SUB`/`CALL`, and closures all need one. Non-local *unwinding*
(`break`, `continue`, `return`) is a separate and much smaller gap — `?` is
already the right mechanism and only a non-error variant of `Error` is missing.

### The decision

Four options were on the table:

| | |
|---|---|
| Give the host a lifetime — `Interp<'i>` | Smallest generator change, but `'i` goes viral through every user's `Value` and every stub, including the majority who never store code |
| A flat owned arena, `Code(u32)` | Storable and `'static`, but inspection is stringly-typed |
| **Generate a typed owned AST** | **Chosen.** The generator already knows every rule's bindings — `views.rs` proves it — so emitting typed nodes is an extension of what exists, not a new mechanism |
| Keep pairs, tell users to build their own AST | Defeats the purpose |

**The parameter pivot (§5.4) is what made this affordable.** Handlers had
already stopped touching `Pair`, so the internal representation was free to
change: the blast radius was five emitters and no user-facing traversal code.
Doing this while handlers still held views would have rewritten every handler in
every project.

### The shape

Every rule-typed field is an `Rc`. One decision buys three things — recursive
types become finite, sharing is free, and a `lazy` binding is storable — where
`Box` plus a separate handle type would have split the model in two.

```rust
pub enum Stmt { Loop(Rc<StmtLoop>), While(Rc<StmtWhile>), .. }

pub struct StmtLoop {
    pub var: Name,              // owned: keeps .text() and .key()
    pub from: Rc<Expr>,
    pub body: Vec<Rc<Line>>,    // the `lazy` body — 'static, storable
    pub span: Span,
}
```

Three things resolve away rather than becoming wrappers: an alias (`rule atom =
primary;`) is a `pub type`, a `-> pass` alternative is typed by what it yields,
and a recovering rule gains an `Error(Span)` variant.

**Operators fold at build time.** `expr` parses as a flat stream and the driver
folds it once, while the tree is built, rather than on every evaluation. That
also fixed a cost noticed in `WHILE`: re-testing a condition was rebuilding its
`OpTree` every pass.

### What it bought

`SUB name .. END SUB` stores its body; `CALL` runs it later, from anywhere:

```rust
host.subs.insert(name.key().to_string(), body.to_vec());
```

That single line is what the whole milestone is for. Cloning a slice of `Rc`
copies pointers, not the program.

And the evidence the design was right: **`examples/config` needed zero handler
changes.** Swapping the entire evaluation substrate left its parameter shapes
untouched. Only the entry point moved, from one call to two — which is a feature,
since the caller now holds a tree it can keep.

### What positional parameters gave up

Views looked children up by **name**: `view.key()` read the child tagged `key`,
wherever it sat in the rule. Parameters are matched by **position**, and that
loses a property nobody noticed until asked how to handle a grammar edit.

| Edit | Views | Parameters |
|---|---|---|
| Add or remove a binding | caught (accessor missing) | caught (arity) |
| Change cardinality or kind | caught (type) | caught (type) |
| Rename a binding | caught (accessor renamed) | **silent** |
| Reorder two same-typed bindings | harmless | **silently wrong** |

The last row is a real defect. Swap two `IDENT` bindings in a rule and the
handler receives them the other way round, with no error and no warning. It was
demonstrated on `examples/config` before the fix: the grammar said `key` was the
second identifier, the interpreter used the first, and the build was clean.

Rust cannot check this — a parameter's name is not part of a call. So the tool
does: `nh build --rust` reads each existing handler's `run` signature and
compares its parameter names against the grammar's bindings. Same names in a
different order is an **error**; different names is a **warning**, because the
values are still right and only the labelling is stale.

Two constraints shaped it. It parses only the specific shape it generates and
returns "no opinion" on anything else, because a false alarm on a handler
somebody rewrote by hand would be worse than the drift it looks for. And it
reports at build time rather than at `nh check` time, since `nh check` never
looks at the Rust side at all.

The general lesson is worth keeping: **when an interface change trades a
compiler-enforced property for ergonomics, the property has to be bought back
somewhere.** It is easy to miss, because everything still compiles.

### The other half: non-local jumps

Storage was one of two gaps. The other is unwinding, and it turned out to be
much smaller: `?` propagation is already the mechanism a `break` or a `goto`
needs, and the only thing missing was a variant of `Error` that is not a
failure.

```rust
#[non_exhaustive]
pub enum Error {
    AlreadyReported,
    Runtime { message: String, span: Option<Span> },
    Signal { label: &'static str, span: Option<Span> },
}
```

The runtime never interprets `label`. It propagates the signal, and reports
against the name if one reaches the top uncaught — which is the reason the
label is a string rather than an opaque tag: `` `break` is not inside anything
that handles it `` is a message worth having for free.

**A value the jump carries rides on the interpreter**, not in the signal.
`nh-runtime` has no idea what a target language's values are, and making `Error`
generic over them would put a type parameter on every `Result` in every project
to serve a minority of languages. `RETURN x` stores and then signals; `GOTO 100`
stores its line number the same way.

`#[non_exhaustive]` came with it: adding a variant to a public enum is a
breaking change otherwise, and more variants are likely.

### `GOTO`, which is what all of this was for

```nh
rule program = SOI EOL* lazy lines:line* EOI -> doc;
rule line = label:NUMBER? body:stmt EOL* -> line;
```

The `program` handler drives rather than folds: it builds a jump table by
reading each line's number **without running the line**, then steps a program
counter, catching `goto` signals and moving it.

Both halves of the original wall are answered. Inspection: `Line` is a typed
struct with a `label` field, where a `Deferred` had no accessors at all.
Storage: the lines outlive any single evaluation, so the driver can return to
one it has already passed.

`IF cond THEN stmt` came along with it, because a backward `GOTO` needs a guard
to terminate — and it is another `lazy` binding, on a single statement.

### What labels turned out to be worth

`EXIT FOR`, `EXIT WHILE`, `EXIT SUB`, `CONTINUE FOR`, and `CONTINUE WHILE` in
`examples/basic-interp` exercise the mechanism, and two properties emerged that
were not obvious when the variant was designed.

**Naming the construct beats naming the action.** Five separate labels rather
than one `"break"` means nesting resolves itself: an `EXIT FOR` raised inside a
nested `WHILE` is not that loop's signal, so it passes through to the loop that
owns it. No depth counter, no unwind bookkeeping, and no way to get the count
wrong. This is the argument for a string label over an enum the runtime knows.

**The label is user-facing.** An uncaught signal reports against it, so
`"EXIT SUB"` produces `` `EXIT SUB` is not inside anything that handles it ``
while `"exit-sub"` would leak a spelling the programmer never wrote into a
message about their own code.

**A handler can also stop a signal.** A `SUB` is a boundary in that example:
`EXIT FOR` inside a subroutine is refused rather than unwinding into whatever
loop happened to call it. Dynamic propagation is the right default — it is what
makes pass-through work — but a construct that is a lexical boundary has to say
so, and it can.

### `lazy` turned out to have two jobs

Functions exposed a second use that the name does not describe:

```nh
| "FUNCTION" name:IDENT "(" lazy params:param_list? ")" EOL*
    lazy body:line*
  "END" "FUNCTION"                      -> function
```

`lazy body:line*` defers **evaluation** — the body runs at each call, which is
what `lazy` was introduced for. `lazy params:param_list?` defers nothing:
parameter *names* are not expressions, and there is nothing there to evaluate.
It is how a handler asks for the node's **structure** rather than its value.

Both are the same mechanism — "hand me the node, not the result" — but only one
is about laziness.

**The name was reviewed and deliberately kept.** The objection is real: in
established terminology *lazy* means **memoised** — Scala distinguishes
`lazy val` (evaluated once, cached) from a by-name `=> T` (re-evaluated at every
use), and this is the second, or weaker still. A `FOR` body runs N times with N
sets of side effects; nothing is cached.

Two alternatives were considered and rejected:

| | |
|---|---|
| `defer` | **Worse.** Go, Swift and Zig all use it for "run at scope exit", which is a specific wrong behaviour rather than a vague one. It also promises the thing *will* run, and a `FOR` with an empty range runs its body zero times |
| `node` / `raw` | **More accurate** — the parameter really is the syntax node rather than its value, which is Lisp's *quotation*. `quote` itself is unusable in a grammar language full of `"literals"` |

What holds `lazy` in place is `lazy(rhs)` in the operator table (§6.6), which
*is* laziness in the ordinary sense — short-circuiting `&&`, which every
language calls lazy. The binding marker was named to match, and both mean "the
handler receives the node", so the consistency is real rather than accidental.
Renaming the marker alone would leave two words for one mechanism.

Deferred pending **usage evidence** rather than more argument: how a keyword
reads is learned by writing grammars with it, not by comparing it to other
languages. Revisit after the language has been used in anger.

### What is still open

Nothing in the M7 programme. `GOTO`, subroutines, functions with parameters and
return values, and loop control all work, with recursion, per-call frames, and
calls inside expressions.

---

## 10. M8 — editor integration

A grammar toolkit is used inside an editor, so the editor is part of the
product. This phase added machine-readable diagnostics to the CLI and a VS Code
extension that consumes them.

### `nh check --json` came first, and is useful on its own

An editor needs structure, not rendered text. `--json` prints one array on
stdout and nothing else, so a consumer never has to strip human output:

```json
{ "severity": "warning", "code": "shadow",
  "message": "this alternative is unreachable: an earlier one matches `let`…",
  "location": { "file": "…", "line": 7, "column": 28, "endLine": 7, "endColumn": 44 },
  "help": "ordered choice takes the first match, so put the longer alternative first",
  "notes": [{ "message": "the earlier alternative is here", "location": { … } }] }
```

Two decisions worth recording.

**The lint name became a field.** It had only ever existed inside a note's
*text* (`note: lint: \`shadow\``), which is fine for a human and useless to a
tool. `Diagnostic::code` now carries it, and the human renderer prints the same
fact from the same source. An editor shows it as the diagnostic code.

**It is hand-rolled rather than `serde`.** The shape is small and fixed, and the
part that can actually break — escaping, since diagnostics are full of
`"literals"` and `\n` — is covered by tests. A JSON dependency in a grammar
toolkit should do more than this to earn its place.

**A note with a location becomes related information.** That is the one place
the JSON shape was designed rather than transcribed: NailHammer diagnostics
routinely name *two* positions — the alternative that shadows and the one it
shadows, the duplicate and the original — and an editor can make that a link.

### Syntax highlighting is a theme concern, not a grammar concern

The most useful thing learned here, because it cost the most time.

A TextMate grammar assigns **scopes**; the **theme** decides what colour a scope
gets. A correct grammar in a theme with no rule for `keyword.control` renders as
plain text, and nothing the extension does to its grammar changes that.

Chasing "no highlighting" produced three wrong answers before the right one:
reload the window (already done), a conflicting extension (none), a bad grammar
(tokenised correctly against the real engine). The decisive fact was the status
bar reading **NailHammer** — the language was active, so scopes *were* being
applied and there was simply nothing mapping them to colours. The theme in use
had nine token rules, every one scoped to its own language, and no base theme to
inherit from.

Two consequences:

- **Prefer scopes themes actually style.** `support.constant` was used for
  preset names and `-> pass`; almost no theme styles it. `constant.language` is
  both semantically right and widely supported, and the swap removed the last
  gap.
- **An extension can supply colours, scoped to one theme.** Two commands write
  and remove `.nh` rules in `editor.tokenColorCustomizations` under the active
  theme's name. Every selector ends in `.nh`, so it cannot affect another
  language, and it does not fight a theme that already styles the scopes.

### Verifying an editor integration

Highlighting is normally checked by looking at it, which means a broken rule
survives until somebody notices a keyword changed colour. Three suites replace
that:

| | |
|---|---|
| `grammar.test.js` | Tokenises real `.nh` with **the same engine VS Code uses** and asserts 36 scopes |
| `language.test.js` | Stubs the `vscode` module and tests the pure logic — indexing a document, deciding what the cursor context is |
| `colors.test.js` | Cross-checks grammar scopes against colour rules **in both directions** |

The third caught a real gap on its first run — `punctuation.separator.nh` had no
colour — and later caught the `support.constant` choice as an outlier. A missing
rule is an uncoloured construct; an extra rule is dead weight that looks like
coverage.

`tsc --noEmit` over JSDoc-annotated JavaScript type-checks against the real VS
Code and Node APIs with no build step, which is the difference between "looks
right" and "calls the API correctly".

### Why no language server

`--json` already returns everything diagnostics need. Completion,
go-to-definition, hover, and the outline run off a regex index of the open
document, because `.nh` declarations are one line each and start with a keyword
— a parser buys nothing a scan does not, and a scan keeps working on a file that
does not currently parse, which is exactly when completion is wanted.

A server earns its place when **cross-file** answers are wanted: completing
rules from an imported fragment, renaming a rule everywhere, finding references.
None of those work today, and none of them can be faked with a scan.

### What the sweep found

Checking that every example was generated, current, and working turned up a real
defect: **two shipped grammars were unanchored.** `example.nh` and
`examples/basic.nh` both had `rule program = stmt+;` — the exact mistake §8.1
describes, in the two files `USAGE.md` points readers at as models.

It survived because **an unanchored grammar fails selectively**:

```
unanchored  "let a = 1;\n"    -> parses
unanchored  "\nlet a = 1;\n"  -> rejected      (keyword-led)
unanchored  "\n1 + 1;\n"      -> parses        (expression-led)
```

`expr` begins with `nh_pre_op*`, and pest skips whitespace around a repetition,
so an expression-led statement tolerates leading trivia by accident. The same
grammar accepts one program and rejects the next.

`nh check` cannot catch this, because nothing in `.nh` declares which rule is
the entry point. `crates/nh-lower/tests/anchoring.rs` lists them and checks
every shipped grammar, and it was verified by un-anchoring one and watching it
fail — a green test that has never been seen red proves nothing.

---

### The trait stack leaned interpreter-shaped

§4.1 claimed an interpreter, a bytecode emitter, and a typechecker were three
impls over one grammar. Building the second one is what showed the claim was
only nearly true.

A bytecode emitter's `Out` is not a value — it stands for something the *target
machine* will compute later — so any trait method that inspects an `Out` is
meaningless to it. There were exactly two, `truthy` and `is_null`, and they were
**required** on `Semantics`. A compiler had to write:

```rust
fn truthy(&self, _: &()) -> bool {
    unreachable!("truthiness is a runtime question, not a compile-time one")
}
```

A method it can never answer and must never be asked. It existed only because
the short-circuit defaults for `&&`, `||` and `??` used it.

**The fix is a split.** `Semantics` is now `type Out` alone — the minimum every
host can meet — and `Values: Semantics` carries the two questions only a host
with values can answer. A compiler simply does not implement it.

That forces one consequence: a Rust default body cannot require a bound its
trait does not have, so the short-circuit bodies cannot stay trait defaults.

### Two wrong answers before the right one

**First attempt: a macro to paste.** The bodies moved to
`nh_value_operators!()`, which an interpreter wrote inside its `Operators` impl,
and the lazy roles defaulted to `unsupported` like every other role.

Measured on the real tree: deleting that line from `examples/calc-interp`
**compiled without a murmur** and failed eight tests at runtime. Forgetting
`impl Values` was a compile error; forgetting the one line that used it was not.

**Second attempt: make it required.** The lazy roles lost their defaults, so
rustc said `missing: or_else, and_then`. Correct, and still wrong — it billed
every interpreter author for a decision nobody makes. `if truthy(lhs) { rhs }
else { lhs }` is not a choice; it is what `&&` *means* for a host with values.
The only host-specific part is `truthy`, which was already written.

The tell was in the documentation. USAGE had grown a paragraph explaining which
line to paste where. **If the docs have to teach a ritual, the generator should
have performed it** (§0).

### The answer: `nh_handlers!` writes it

The lazy roles moved to their own trait, `ShortCircuit`, for one reason — a
separate trait means a separate `impl` block, and `nh_handlers!` can write that
without touching the `Operators` impl the user hand-writes.

```rust
nh_handlers!(Interp);                          // Handlers + ShortCircuit, from your `truthy`
nh_handlers!(Compiler, without short_circuit); // I emit code; I'll write my own
```

Every property holds at once:

| | interpreter | compiler |
|---|---|---|
| short-circuit code written | **none** | its own `impl ShortCircuit` |
| forgets `impl Values` | compile error | n/a — never needed |
| forgets the impl after opting out | n/a | compile error |
| pays for the other shape | no | no |

`ShortCircuit`'s methods are still *declared* rather than defaulted, because
there is no correct default. But nobody meets that declaration except a host
that said `without short_circuit` — for whom "write it yourself" is the whole
intent. The common case now costs zero lines, down from two.

The comma in `(Interp, without short_circuit)` is not styling: Rust's macro
follow-set forbids a bare word after a `ty` fragment.

### §0 applied: the run loop

Codifying the principle immediately turned up a bigger violation than the one
that prompted it. Every project hand-wrote the same seven steps — parse, render
a parse error, collect recovered syntax errors, seed a `Ctx`, build the tree,
evaluate, decide an outcome. None is a decision. All were easy to get wrong.

They *were* wrong. Six of the eight parse sites in this repository's own
examples and tests built a tree and never checked for recovered syntax errors,
so a program with a reported typo ran anyway:

```
examples/config/src/main.rs
examples/basic-interp/tests/run.rs
examples/bytecode/tests/compile.rs        <- written the same week
examples/calc-interp/tests/ast.rs
examples/config/tests/interpret.rs
examples/basic-interp/tests/ast.rs
```

`config/src/main.rs` also printed raw pest errors, having quietly missed
`render_parse_error`. Nobody had made a bad decision; the sequence was simply
long enough that a copy drifted.

**Generated: `generated/run.rs`.**

```rust
pub fn eval_source<H: Handlers>(host: &mut H, cx: &mut Ctx, file: FileId)
    -> Result<H::Out, Vec<Diagnostic>>
```

**Not generated, deliberately.** Loading the source, because a file, a socket
and a string literal in a test are all legitimate. Formatting a diagnostic,
because where errors go is a property of the program, not of the grammar — a
binary prints, a test asserts, an editor draws squiggles. Returning the list
lets all three share one path. Both ends are scaffolded by `nh init`, for either
shape, so nobody writes them from nothing.

Two things fell out of building it:

* **Duplicate diagnostics.** Evaluating an error node reports the same syntax
  error `syntax_errors` already collected. Every reached recovery point printed
  twice until the driver deduplicated. `syntax_errors` is the copy kept, being
  complete — it sees error nodes in code that never ran.
* **`Error::diagnostic()`.** `Ctx::render` built a `Diagnostic` and immediately
  threw away the structure. Splitting it out is what lets a terminal failure
  join the same list as the syntax errors.

`parser_type` also stopped being a caller's job. `grammar Calc;` implies
`CalcParser`, so `generate` derives it rather than asking — the same rule, one
level down.

### `nh init --compiler`

The scaffold shipped one shape, which made "one grammar, two shapes" something
you had to take on trust. `--compiler` writes the same grammar with handlers that
emit and its own `ShortCircuit`.

It began as a stack machine — `type Out = ()` — which is where the section below
picks up; it is a register machine now. `examples/bytecode` keeps the stack
version, because a stack machine shows the point in fewer moving parts.

An `#[ignore]`d e2e test builds every style × feature × shape combination and
asserts the two shapes print the same thing. If they ever diverge, something has
become interpreter-shaped that should not be.

### The two shapes disagreed about a language question

Found by poking at the scaffold rather than by a test. Reading a never-declared
name:

```
             interpreter                   compiler
--style c    error: undefined variable `x`  0
--style basic error: undefined variable `x` 0
```

The compiled program computed a different answer from the interpreted one, for
the same source. That is precisely the divergence §4.1's whole claim rests on
not happening.

Two mistakes, stacked:

1. **It was treated as a host detail** rather than a language decision. The
   interpreter's `primary_var` handler chose to error; the VM's `Load` chose to
   default. Nobody decided; two files drifted.
2. **The style axis was ignored.** Zero is *correct* for BASIC, which has always
   started every variable at zero. Erroring is correct for a language where
   declaring is deliberate. So there is no single right answer — only a right
   answer per style, which then must hold across both shapes.

The fix moves the decision onto the host, next to the symbol table, and out of
the handler:

```rust
// handlers/primary_var.rs — shared by both styles
host.read(name.key(), cx)
```

`--style c` errors in both shapes; `--style basic` reads zero in both.

### Runtime errors in compiled code went to stdout

Next to it, and less defensible. The scaffold VM reported an undefined function
by pushing `"error: undefined function ..."` into the program's **output** and
returning normally — a diagnostic on stdout, exit code 0, indistinguishable from
data to anything downstream. The interpreter had always reported properly.

`run()` now returns output *and* an optional error, so a failure reaches stderr
and the exit code while whatever the program managed to print still appears.
Both halves matter: a partial run is worth seeing, for the same reason `main.rs`
prints before it checks the outcome.

### The compiler scaffold is a register machine

`type Out` earns its keep here. An interpreter's is a value; a stack compiler's
is `()`, because nothing is returned. A register compiler's is **which register
holds the result** — and the operator trait then reads as three-address code
with no change to the toolkit:

```rust
fn add(&mut self, a: Reg, b: Reg) -> Result<Reg>
```

Two findings, in order, because the first was nearly a wrong conclusion.

**Registers alone bought nothing.** The first prototype kept variables in a
name-keyed map and used registers only for expression temporaries. Same program,
**100 instructions either way**. The textbook "four dispatches versus one"
assumes the operands are already in registers; when every variable access is a
hash lookup, both shapes pay it.

**Slots are what pay.** Giving each function a compile-time symbol table —
parameters at slots `0..n`, locals at the next free slots, globals still by name
— changes the picture completely. The same function:

| | instructions | name lookups |
|---|---|---|
| stack | 33 | 11 |
| register + slots | 18 | **0** |

Per loop iteration it is 8 against 17. Reading a local now emits **no
instruction at all**: `primary_var` hands back the slot.

Three things fell out that are worth knowing:

* **Calling conventions are free.** Arguments must be in consecutive registers;
  they already are, because eager parameters evaluate left to right into an
  allocator that hands out the top of the file. A `debug_assert` guards it and
  has never fired.
* **`free` must skip locals.** A local's slot belongs to it for the whole
  function. Getting this wrong corrupts a live variable and produces wrong
  answers rather than a compile error — the counting loop's limit register
  caught it during the prototype.
* **The handlers got *smaller*** — 255 lines against 262 — because the allocator
  lives on the host. `stmt_for` never asks whether its counter is a slot or a
  global; `read_var` and `emit_increment` answer that.

What is deliberately not done: locals are function-scoped rather than
block-scoped, and there is no peephole pass, so `x = x + 1` still emits an `Add`
into a temporary followed by a `Move` to the slot. Folding that pair is about a
quarter of the loop body and is the obvious next optimisation.

### The AST dictated a threading model

`Rc` in every rule-typed field. Which meant a parsed program was **not `Send`**,
so it could not cross a thread boundary at all — no parsing on one thread and
running on another, no sharing a stored function body between workers, no VM used
from a work-stealing pool. Measured, not assumed:

```
error[E0277]: `Rc<Program>` cannot be sent between threads safely
```

Everything *else* was already fine: `Span`, `Name`, `Diagnostic`, `Error`, `Ctx`
and `SourceMap` are all `Send`, and the runtime contained no `Rc` at all. The
constraint was one type, in generated code.

**Neither pointer is right in general**, which is what made it a dictate rather
than a bug. A single-threaded interpreter should not pay for atomic refcounts it
never needs; a compiler that emits on another thread cannot use `Rc`. So it is a
cargo feature on `nh-runtime`, off by default.

The part worth recording is *where the choice is spelled*. Substituting `Arc` for
`Rc` throughout the emitters would have worked, and would have meant every
handler taking a `lazy` binding had to be rewritten when the feature flipped —
churn with no meaning, in files the user owns, for a decision made in a manifest.
Naming the alias instead:

```rust
pub type Shared<T> = std::rc::Rc<T>;      // default
pub type Shared<T> = std::sync::Arc<T>;   // `threadsafe`
```

means generated code and handlers both say `Shared<T>` and **no signature moves**.
An e2e test scaffolds a project, asserts the tree is *not* `Send`, changes one
line of `Cargo.toml`, and asserts it now is.

The rename cost one bug, and it is a good example of its kind: `qualify_rc` found
`"Rc<"` and then skipped `3` characters. With a 7-character needle it cut into the
middle of the word and emitted `ast::red<Stmt>`. The length is taken from the
needle now — a magic number should not be able to do that.

### A language with its own futures: suspend, do not await

The obvious reading of "my language needs `await` in an expression" is that the
evaluator must become async. It is the wrong reading for a compiled host, and the
right answer costs nothing.

A tree-walking interpreter has only two options, and both are bad:

* **Block** on the future. Needs a multi-thread runtime — `block_in_place` panics
  on a current-thread one — costs a worker thread per await, and if the *language*
  has concurrency, blocking one program blocks the interpreter entirely.
* **Async evaluator.** Every `eval_*` returns a boxed future, because async
  recursion requires `Box::pin`. A heap allocation per node, whether or not a
  language ever awaits.

A compiled host has a third, and it is what every real async VM does: `await` is
an **instruction**, and the machine **suspends** rather than awaiting.

```rust
pub enum Step {
    Done,
    Failed(String),
    Awaiting(f64),   // waiting on this; resolve it and `resume_with`
}
```

The machine never touches a runtime. Whoever drives it does the waiting, so the
same bytecode serves a blocking loop with no runtime at all, a multi-thread tokio
host, and a single-threaded one — verified for all three, including two `AWAIT`s
inside one expression with precedence preserved.

For the grammar author this is three lines of grammar and three of host code,
because the operator table already routes a prefix word to a role and the role
already escapes a Rust keyword to `r#await`.

**What had to change, and why it had to change now.** Nothing about `Await`
itself — an opcode is one enum variant anyone can add. It was where the machine
kept its state. `run()` held `pc`, the frames and the globals in local variables,
and a loop like that cannot be stopped and started at all; converting it is a
rewrite of the interpreter. So the scaffold keeps them in a `Machine` struct
whether or not a language ever suspends. It costs nothing at run time, and it is
the only part that cannot be added afterwards.

`run()` survives as a convenience for programs that never suspend, and refuses
rather than guessing:

```
error: this program suspends; drive `machine()` instead
```

### The compiler became the default, and `--async` went away

Two decisions that turned out to be one.

`--async` added tokio and a `block_on` helper. Its only purpose was papering over
a tree-walker's inability to suspend, and both ways of giving one async are bad
(above). Offering the less-bad one is worse than offering neither: it reads as a
supported path. So the flag is gone, and a test asserts **no scaffold mentions a
runtime** — not tokio, not `block_in_place`, not `async fn` — for either shape.
Somebody who wants to block inside an interpreter handler adds four lines
themselves. Being able to is different from being handed it.

With that gone, the shapes are no longer symmetric: one can suspend and one
cannot, and the one that can is also faster. So `nh init` scaffolds the compiler
and `--interpreter` opts out, rather than the other way round. The tree-walker
stays because it is the shorter path to a working language and much the easier one
to read — not because it is the one to build on.

The interactive picker asks shape first, since style and feature set sit inside
that choice.

### Async is offered, not assumed

*Superseded by the section above: the flag is gone.* It assumed tokio, its
multi-thread flavour specifically, and sync-over-async at the cost of a worker
thread — defensible for a handler that occasionally reaches the network, and the
wrong shape to hand anybody as *the* answer.

The evaluator stays synchronous for the reason it always did: an async one means
every `eval_*` returns a boxed future, a heap allocation per node, whether or not
a language ever awaits anything.

### What stayed different, and should

Not everything that differs is a defect. Non-local control flow legitimately
diverges: an interpreter unwinds with `Error::Signal`, a compiler emits a jump
and records its index for patching. That is host state rather than a signal, and
no shared mechanism would serve both — patching is not unwinding. Nothing in the
generated code forces either choice.

Both shapes are now built from the same grammar and shipped:
`examples/bytecode` is the scaffold grammar, unchanged, with `type Out = ()`.
`2 + 3 * 4` compiles to `Push 2 · Push 3 · Push 4 · Mul · Add`, and
`if x then print 100` to a `JumpIfFalse` patched once the body's length is
known. Eager parameters give a compiler stack order for free, because "already
evaluated" reads as "already emitted".

That it lives in `examples/` rather than a scratch directory is the point:
`tests/compile.rs` asserts on the instruction stream, so the next change that
assumes an interpreter fails in CI rather than in somebody's project.

---

### Recovery ate its own terminator

`recover stmt sync ";"` lowered to

```text
nh_error_stmt = { (!(";") ~ ANY)+ ~ (";")? }
```

— consume anything that is not the sync token. Including the `}` that closes
the block the statement is inside. At the closing brace `stmt`'s real body
fails, the error node matches, and it eats the brace; `stmt*` never terminates
and the block never closes.

**It broke every grammar with a block**, which is most of them. The three
examples here escaped because they recover only at the top level, where the
closer is `EOI` and `ANY` stops there anyway. Nothing caught it until `nh init`
grew an `if` with a braced body.

Two things made it nastier than the one-line cause suggests:

* **The symptom named the wrong thing.** The user saw *"could not parse this
  `stmt`"* pointing at their `if`. Nothing anywhere said "recovery".
* **A test that only checked "did it parse" passed against it.** `program` still
  matched — recovery swallowed the whole file into one error node and the
  top-level `stmt*` was content. The regression tests in
  `crates/nh-lower/tests/recovery.rs` assert on *rule names in the tree*, and
  four of the seven fail if the fix is removed. Written the obvious way, one
  did.

**The fix** derives what recovery must stop at, rather than asking:

```text
nh_error_stmt = { (!(";") ~ !(nh_kw_else) ~ !(nh_kw_while) ~ !("}") ~ ANY)+ ~ (";")? }
```

`follow.rs` collects every terminal that can follow the recovered rule *or any
rule that transitively contains it*. Transitivity is the part that is easy to
get half-right: in the line-oriented style the chain is `stmt` → `line` →
`block` → `WEND`, and stopping at any depth leaves a loop eating its own
terminator. A rule reference expands to what that rule can *start* with, so the
guard is `!(nh_kw_else)` rather than a lookahead that re-parses a whole block —
and a reserved word keeps its boundary guard, so `wendy` is still a variable.

Asking the author to list the closers would have been the boilerplate §0 exists
to remove: the grammar already says where its blocks end.

The fix is inert where it was not needed — no shipped `.pest` changed.

## 11. Resolved questions and residual risks

**All open design questions are closed.** Resolved across v0–v0.4: handler return
type, multi-pass strategy, precedence surfacing (dissolved by §6.3), keyword
handling, span scope, case folding (§5.3), `Place` representation (§6.8), postfix
shapes (§6.7), error poisoning (§5.5), grammar imports (§3.1), and operator
literal ordering (§5.2).

What remains is not undecided design but **risk that only implementation can
retire.** Recorded so the first surprise isn't a discovery:

### Open

*(1 is retained in place so the numbering below stays stable.)*

1. ~~**`.nh` cannot guard a literal without reserving it.**~~ *Closed.*
   `guard from TOKEN { .. }` is `reserved from` minus the rejection: the literal
   gets an identifier-boundary lookahead, and stays usable as an identifier.

   The self-hosting grammar was the proof — its 29 hand-written
   `token KW_X = @ "x" !IDENT_TAIL;` rules collapsed to one declaration, and it
   still parses every `.nh` file in the repo. Adding the feature also made the
   grammar stop parsing until `nh.nh` learned to describe `guard_item`, which is
   self-hosting doing exactly what it is for.

2. ~~**Handler stub churn.**~~ *Retired.* `nh build --prune` removes orphaned
   handlers, and the orphaned/stale distinction turned out to be
   *never-implemented* versus *contains real code* — decidable from the stub's
   `compile_error!` marker, which the stub itself instructs you to delete.
   Signature drift within a *current* handler is still only caught by the
   compiler, which is the right place for it.

3. ~~**Identifier-continuation derivation is a heuristic.**~~ *Closed.* It is
   still a heuristic — reading the operands of the token's repetitions, which is
   exactly right for an ordinary identifier — but it no longer approximates
   *silently*. A token with no repeated tail now produces a warning naming the
   token and suggesting `boundary TOKEN = <what may follow>;`, which states the
   class outright and takes precedence over the derivation.

   Fixing this also exposed that **`nh-lower` had no way to report a non-fatal
   finding at all** — its warnings were computed and dropped. `Lowered` now
   carries them and both `check` and `build` print them.

4. ~~**Grouped-role discriminants across imports.**~~ *Closed, and it was not
   the problem I expected.* Two tables binding `compare` with different operator
   sets is **fine** — the discriminant unions them, which is how an imported
   table extends another.

   The real defect was next door and worse: binding one role at two *fixities*
   emitted two trait methods of the same name, so the generated code did not
   compile — with an error pointing at generated Rust rather than at the grammar
   that caused it. A role names one operation with one signature, so that is now
   a table error naming both locations. (`-` was never an example of this: it is
   `sub` infix and `neg` prefix, two roles.)

5. ~~**`.nh` cannot express silent or compound-atomic rules.**~~ *Closed, and
   compound-atomic needed no new syntax at all.*

   `token X = @ body;` was already atomic; `token X = body;` emitted a plain
   `{ }`, which meant **implicit whitespace was skipped inside a token** —
   `token WRAPPED = "<" INNER ">";` matched `< abc >`. That is never what
   `token` means. Non-atomic tokens are now compound-atomic (`${ }`): still no
   whitespace, but inner rules keep producing nodes.

   So one change closed the gap and fixed a latent bug, and `token` became
   coherent: it always means "no implicit whitespace", and `@` additionally
   means "no inner structure". No shipped grammar used the old form, so nothing
   broke — but any that had would have been silently wrong.

   Adding silent rules surfaced a constraint worth stating: **silence and
   bindings are mutually exclusive.** A silent rule produces no node, so nothing
   can bind it — pest rejects the tag, with an error pointing at generated
   `.pest` and naming no grammar line. The `silent-binding` lint catches it in
   the grammar instead. It is also why `nh.nh` uses no silent rules despite
   having several pure alternations: it binds almost everything, because it is
   meant to be usable for code generation rather than only recognition.

6. **`.nh` has no bounded repetition.** *New at M6, narrowed since.* `*`, `+`,
   and `?` only — no `{n,m}`. `nh.pest` uses `ASCII_HEX_DIGIT{1,6}` for unicode
   escapes, which is the last construct self-hosting cannot reproduce. Writable
   longhand today, and the only remaining reason M6 asserts language
   equivalence rather than textual reproduction.

7. **Recovery does not compose with block-structured rules.** *New at M7.* An
   error rule matches any text up to its sync point, so a recovering rule used
   inside a bounded repetition swallows the token that closes the block:
   `rule block = "{" body:stmt* "}"` with `recover stmt` eats the brace and
   `stmt*` never terminates. `examples/basic-interp` deliberately ships without
   recovery for this reason. The fix is a FOLLOW-set computation the analyser
   does not do — the error rule would have to decline to match anything that can
   close an enclosing construct.

8. **Cross-file editor features need a language server.** *New at M8.* The
   extension indexes the open document, which is enough for completion,
   hover, definition, and the outline within one file. Completing a rule from an
   imported fragment, renaming a rule everywhere, and finding references all
   need a real resolve step, and none of them can be faked with a scan (§10).

### Standing constraints

6. **Generated code must not warn.** *New at M3, recurred at M5.* A degenerate
   generated function — `resolve_place` for a grammar with no places, `describe`
   for one with no `expect` labels — carries unused parameters and single-arm
   matches: clippy warnings in a file the user does not own and cannot fix.
   Every emitter special-cases its empty shape. **Anything generated is read by
   the user's linter, and a warning they cannot act on is a defect.**

7. **A lint that fires on working code is worse than no lint.** *New at M4.*
   Enforced by `the_shipped_grammars_produce_no_diagnostics`: every grammar in
   the repo must analyse completely clean. Any new analysis needs the same gate.

8. **A scaffold rots faster than documentation.** *New after M5.* `nh init` sat
   four milestones out of date while every test passed, because the tests
   checked file existence rather than behaviour. Its end-to-end tests now
   compile the generated project and assert its output.

9. **A generator that hands you a tree has moved the tedium, not removed it.**
   *New after M6.* Handlers originally received a *view* and fetched their own
   inputs. Named accessors were a genuine improvement over `into_inner().nth(2)`
   and passed every test, so the interface looked finished — but every handler
   still opened with the same three lines of traversal, and `into_pair` was a
   word the reader had to go and learn. The measure is not "is the access
   type-safe"; it is **"is there anything left for the handler to do that the
   generator already knows how to do?"** Bindings became parameters (§5.4), and
   the example handlers lost roughly half their lines.

   The corollary is the harder half: eager parameters break constructs that must
   *not* evaluate their body, which is where `lazy` came from (§6.6). Removing
   ceremony is only correct if you also carry across what the ceremony made
   possible.

10. **Content-hashed temp paths collide; they do not prevent collisions.**
    *New after M6.* Two test helpers named their scratch grammar by content
    hash, with a comment claiming this stopped concurrent tests colliding. It
    guaranteed the opposite: tests using *identical* grammar text got the same
    path, and one truncated the file while another read it. The symptom was an
    intermittent "no `grammar` declaration found" in whichever test lost the
    race — a message pointing nowhere near the cause. Uniqueness has to come
    from a counter, not from the content.

11. **"Generated code must not warn" includes warnings from tools that are not
    rustc.** *New after M6.* A grammar using `operators::none` was handed the
    whole operator driver — `expr = { atom }`, `ExprView`, precedence tables,
    `eval_tree` — all unreachable. Rustc said nothing, because the generated
    file carries `#![allow(dead_code, unused_imports)]`, and clippy was clean.
    The **pest language server** flagged the unused rule in the editor, in a
    file the user owns and cannot edit. `expr` is now emitted only when the
    table has operators or the grammar binds it; `config`'s dispatch dropped
    from 338 lines to 259. The blanket `allow` had been hiding the evidence, so
    the check has to be "is any of this reachable", not "does rustc complain".

12. **A green test that has never been seen red proves nothing.** *New at M8.*
    The anchoring test passed on its first run — and would have passed with the
    bug still present, because the sample program it used was expression-led and
    those parse unanchored by accident. It was only trustworthy after
    deliberately un-anchoring a grammar and watching it fail. The same pattern
    caught two other tests in this session asserting the wrong thing: one
    checked column 0 instead of where the cursor is, and one used a substring
    (`Deferred`, `Name`) that a user's own label could collide with.

13. **Verify what the user sees, not what the code says.** *New at M8.* Syntax
    highlighting was correct in every mechanical sense — right scopes, right
    engine, right install — and invisible, because the theme mapped none of the
    scopes to colours. Nothing in the extension could have detected that; the
    decisive evidence was one word in the status bar. When a mechanism is
    verified and the outcome is still wrong, the missing piece is downstream of
    everything under test.

14. **When an interface change trades a compiler-enforced property for
    ergonomics, buy the property back.** *New after M7.* Views looked children
    up by name, so reordering a rule's bindings was harmless. Positional
    parameters are better in every other respect and silently swap two bindings
    of the same type. Rust cannot check a parameter name across a call, so
    `nh build` does: same names in a different order is an error, different
    names is a warning. This is easy to miss precisely because everything still
    compiles.

### Retired

15. ~~**`OpTree` borrow lifetimes.**~~ *Retired at M3.* The tree borrows input,
   not interpreter state, so threading `'i` through a mutable fold never
   conflicted.

16. ~~**`Thunk` and `&mut self` together.**~~ *Retired at M3, then dissolved at
    M7.* `Deferred` borrowed the tree rather than the host, so forcing it inside
    a method already holding `&mut self` was two disjoint borrows. The owned AST
    removed the question entirely: an operand is an `Rc<Expr>` and borrows
    nothing.

17. ~~**Tag survival through silent rules.**~~ *Retired.*
    `nested_binding_does_not_leak_to_the_enclosing_node` covers the direct-child
    scan, and M5 proved the converse case in anger: a *non*-silent `expect` rule
    changed a node's child count and broke transparent delegation, which the
    existing tests caught immediately.

18. ~~**`OpTree` and error recovery interaction.**~~ *Retired at M6.* An error
    node is just an operand to the builder, and dispatch poisons it during
    evaluation. A malformed operator stream returns `BuildError`, never panics —
    `a_trailing_operator_is_an_error_not_a_panic` holds the line.

19. ~~**There is no `start` declaration.**~~ *Mitigated by `nh init`.* Anchoring
   the entry rule with `SOI`/`EOI` is still the author's job, matching
   hand-written pest — but the scaffold generates an anchored rule with a
   comment explaining why, and a sample program that begins with a comment and
   would fail without it. A test asserts both. Encoding the convention in a
   template beats documenting it, because the failure is silent and nobody
   reads a guide closely enough to prevent a silent failure they have not met
   yet. An explicit `start` declaration remains open but is no longer urgent.
