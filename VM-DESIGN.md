# An extensible VM, and what languages must agree with it

**Status: §7 is built and shipping as the `nh-vm` crate; §1–§6 are superseded
reasoning, kept deliberately; §8's published contract is designed, not built.**

What exists: `Op<X>` with a language's own instructions in `Ext`, a `Machine`
with call frames and `Step::Awaiting` suspension, `snapshot`/`restore`, the
`SharedStore` trait with five implementations, the `Emitter` trait a compiler
implements in two methods, and a versioned wire format. `nh build --target
nh-vm` generates the `Operators` impl rather than making you write it, and
`examples/vm-c` and `examples/vm-basic` are two unrelated languages proved to
emit identical bytecode.

What does not exist yet is §8.3's **published configuration** — the file a VM
owner ships so a language developer can read the contract, and the
`nh explain --config` that would print it. Where this document shows that flag,
it is describing a design, not a command you can run.

How a pluggable interpreter system can accept languages it has never seen.

**Read §7 first.** This document was written forwards and the argument moved.
§1–§6 propose a *file format* for describing many different machines, and are
kept because the reasoning is the trail that leads to §7 — including the parts
that turned out to be wrong. §7 replaces the premise: rather than describing
machines that differ, ship **one extensible VM** and let languages add commands
to it. §8 works out what that means for plugins, vendoring, and what a language
developer actually has to agree with.

What survives from §1–§6 is noted where it survives. The rest is superseded, and
marked.

---

## 1. The problem

`nh init` writes a `src/lib.rs` into the user's project and hands them ownership
of it. That one file contains all four of:

```rust
pub enum Op { .. }        // the opcode set
pub struct Interp { .. }  // the compiler
pub enum Step { .. }      // the suspension contract
pub struct Machine<'a>    // the VM
```

So **every language built with NailHammer invents its own bytecode and ships its
own interpreter.** Two NailHammer languages produce mutually unintelligible
output. That is exactly right for someone building a standalone language, and
exactly wrong for a host that wants to load front ends as plugins.

The fix is not to pick a bytecode and bless it. It is to let a machine *describe
itself* in a file, and let a front end name that file.

The standing principle applies without modification. Once a VM owns execution,
"the `add` role emits `Add`" is not a decision the language author makes — it is
a consequence of targeting that VM. It should be generated, and the thing it is
generated *from* is the target file.

This also settles the layering: **NailHammer never learns what Crash BASIC is.**
It learns what a target file is. Crash BASIC ships one; so can anyone. Presets
already work this way (§6.1: presets are ordinary `.nh` tables with no special
powers), and a target that had privileges a third party could not obtain would
be the same mistake in a new place.

---

## 2. What two real machines prove

The repository already contains two machines with incompatible shapes. Neither
was written to make this point, which is what makes them worth using: any format
that cannot express both is wrong on evidence rather than in theory.

**`examples/bytecode` is a stack machine.** Operands are implicit:

```rust
pub enum Op { Push(f64), Add, Sub, Neg, Pop, Dup, JumpIfFalse(usize), .. }
```

**The scaffolded compiler is a register machine.** Operands are named:

```rust
pub enum Op {
    LoadK { dst: Reg, value: f64 },
    Add { dst: Reg, a: Reg, b: Reg },
    Compare { dst: Reg, op: CompareOp, a: Reg, b: Reg },
    JumpIfFalse { src: Reg, target: usize },
    ..
}
```

Three requirements fall out of the difference, and a fourth out of what they
have in common.

### 2.1 The machine model has to be declared

`add` on the stack machine consumes two operands and pushes one; no operand is
ever named. On the register machine it is `Add { dst, a, b }` and every operand
is named. The same role, structurally different emission. A format that assumes
either one cannot describe the other.

### 2.2 Lazy roles are sequences, not instructions

`and_then` in the stack machine:

```
  Dup
  JumpIfFalse → L        (target unknown until rhs has been emitted)
  Pop
  <rhs>
L:
```

`Dup` is there because if `a` is falsy it *is* the result, so the test must not
consume it. No opcode column expresses that. The format needs sequences with
**forward patch points**.

### 2.3 A result convention is needed, and only on some machines

The register machine's `and_then` is not merely a different sequence — it ends
somewhere:

```rust
let skip = self.emit_jump_if_false(lhs);
let r = rhs.eval(self, cx)?;
self.emit(Op::Move { dst: lhs, src: r });   // <- put the result where lhs was
self.free(r);
self.patch_to_here(skip);
Ok(lhs)                                      // <- and the answer is lhs
```

Both arms have to leave the value in the *same* place or the code after the
label cannot name it. On a stack machine that is free — both arms leave it on
top of the stack. On a register machine it is a `Move` and a decision about
which register wins.

### 2.4 Grouped roles carry a discriminant

`compare` is one role covering six spellings. The stack machine would need six
opcodes; the register machine has one, `Compare { op: CompareOp, .. }`, taking
the discriminant as an operand. The format has to allow either.

---

## 3. The format

A target file is declarative and is parsed by `nh-syntax`, for the same reason
presets are: so that "a target has no special powers" is true in the
implementation and not only in this document. It also avoids taking a TOML or
serde dependency for the sake of one file.

### 3.1 Header

```
target  crash-basic;
format  1.0;
machine register;        // or `stack`
```

`format` versions the *file contract*, independently of the VM's own version, so
a front end can say which VMs it is compatible with without tracking VM
releases.

### 3.2 Simple roles

One role, one instruction. Holes are filled by the driver:

```
// register
op add     = Add     { dst, a: lhs, b: rhs };
op sub     = Sub     { dst, a: lhs, b: rhs };
op neg     = Neg     { dst, a: operand };
op compare = Compare { dst, op: discriminant, a: lhs, b: rhs };

// stack — operands are positional, so there is nothing to name
op add     = Add;
op sub     = Sub;
op neg     = Neg;
```

The names `lhs`, `rhs`, `operand`, `dst` and `discriminant` are the only holes.
They are fixed by the role's fixity and by whether the role is grouped, both of
which NailHammer already knows from the `.nh` table — which is why the target
file does not restate them.

A grouped role that the machine spells as separate opcodes writes them out:

```
op compare[lt] = Lt;
op compare[le] = Le;
op compare[eq] = Eq;
```

### 3.3 Lazy roles

A sequence, with labels for patch points and `eval` for "emit the operand here":

```
// register
lazy op and_then {
    result lhs;                          // the answer ends up where lhs was
    JumpIfFalse { src: lhs } -> done;
    eval rhs -> r;
    Move { dst: lhs, src: r };
  done:
}

// stack
lazy op and_then {
    result stack;                        // both arms leave it on top
    Dup;
    JumpIfFalse -> done;
    Pop;
    eval rhs;
  done:
}
```

Four constructs, and deliberately no more:

| | |
|---|---|
| `result <place>` | where the value is when the sequence ends |
| `<Op> { .. }` | emit one instruction |
| `eval <operand> [-> <name>]` | emit the operand's code, optionally naming its result |
| `label:` and `-> label` | a forward patch point |

There is no arithmetic, no conditionals, and no loops. That boundary is the
whole design: it is enough for short-circuit, null-coalescing and ternary, and
stopping there is what keeps this a file format instead of a compiler backend
language.

### 3.4 Literals

```
literal number = LoadK { dst, value };   // register
literal number = Push(value);            // stack
```

### 3.5 Semantic configuration

`JumpIfFalse` means the machine has an opinion about truth. So does every VM
that has a conditional jump, and a language whose truthiness differs cannot
quietly target it — the jump would be taken on different values than the author
believes.

The answer is not to give the language somewhere to disagree, nor to make it
restate the rules so a checker can compare them. It is for the machine to
**declare them once**, and for everything compiled against that machine to
inherit them.

```
// in the target
values { number; string; bool; null; }

semantics {
    truthy {
        number = nonzero;
        string = nonempty;
        bool   = self;
        null   = never;
    }
}
```

`truthy` is a **group** inside `semantics` rather than a keyword of its own,
because it is not the only question of its kind — §3.6 scopes out the rest.

The predicate vocabulary is closed — `always`, `never`, `nonzero`, `nonempty`,
`self` — for the same reason the sequence constructs in §3.3 are. A rule that
cannot be said in five words is the VM's business and does not belong in a file
the VM's users write.

**The grammar states nothing.** An earlier draft let a `.nh` file restate the
same group, so the tool could check the two against each other and error if they
disagreed. That block is removed, and the reasoning is the same one that removed
the config identity (§8.3) and the index knob (§3.7):

A declaration that can only ever *agree* is not a declaration. Its sole function
is to be checked, which makes it a guard — and it earns its cost only against a
developer who would otherwise get it wrong silently. There is no such developer
here. The VM publishes what is truthy; the language inherits it; a person who
wants to know reads the contract.

It also had the usual tail of edge cases already showing: what if the grammar
lists only some types, what if it lists a type the VM does not have, which file
does the error blame. All of that for a restatement nobody needed to write.

So truthiness is declared **once**, by the machine that implements it, and read
by whoever wants to know:

```console
$ nh explain --config crash-basic
truthy:  number = nonzero   string = nonempty   bool = self   null = never
```

#### This is a compiler concern, and only a compiler concern

**A tree-walking interpreter declares none of this and should not be asked to.**
`Values::truthy` stays hand-written Rust, free to say whatever the language
means, over a `Value` enum NailHammer never has to know about.

That is not an omission to fix later. Freedom about semantics is what the
interpreter shape *is*: it evaluates the tree directly, so truth can be anything
the author can write in a method, including something no five-word vocabulary
could express. Taking that away to make the two shapes symmetrical would remove
the reason to choose one.

The freedom is paid for, and the price is already documented. A tree-walker is
slower, and it **cannot suspend** — `Step::Awaiting` exists only in the compiled
shape, which is why USAGE has a section titled *"A tree-walking interpreter has
no async story."* The constraint in this section is what the compiled shape
spends to get performance and suspension: agreeing with a machine about what
things mean is the cost of not being the machine.

So the rule is scoped by shape, not by preference:

| Shape | Truthiness |
|---|---|
| interpreter | hand-written, unconstrained, no target |
| compiler with `--target` | declared, checked against the target, closed vocabulary |
| compiler without a target | its own VM, so its own business — like an interpreter |

A grammar carrying a `truthy` block and no target is not an error; it simply has
nothing to be checked against, and generating a `truthy` from it would require
the grammar to declare its value types, which nothing here asks for.

---

### 3.6 `semantics`, scoped out

Truthiness is not a special case, so before adopting a keyword for it, here is
what the whole category looks like if every candidate is written down:

```
semantics {
    truthy {
        number = nonzero;
        string = nonempty;
        bool   = self;
        null   = never;
    }

    number {
        division  = truncating;    // truncating | euclidean | rational
        overflow  = wrap;          // wrap | saturate | trap | promote
        div_zero  = trap;          // trap | infinity | nan
    }

    compare {
        mixed    = never_equal;    // never_equal | coerce | trap
        equality = structural;     // structural | identity
    }
}
```

An `index { base = 0 | 1; }` group belonged here in an earlier draft and has
been **removed on purpose**: indexing is 0-based, full stop, and a language
targeting this VM is 0-based too. §3.7 argues why deleting the knob was the
better move than supporting both.

#### Does it fit the paradigm?

Four tests, and it passes three cleanly.

**Closed vocabulary — yes.** Every key takes one of an enumerated set. Nothing
here is an expression, and nothing needs to be. Same discipline as §3.3.

**Assert-only — yes**, now that indexing is mandated rather than configured.
An earlier draft found a split here: some semantics the front end must *match*
because it cannot compensate (`truthy` — `JumpIfFalse` tests what the VM says it
tests), and some it could *bridge* (`index.base` — a 0-based language on a
1-based VM is an `Add 1` at every index site, not an error). That distinction
would have had to be carried by the format, per group, forever. Deleting the
only bridgeable group deleted it instead (§3.7).

**The standing principle — yes.** "Does this VM trap on overflow?" is not a
question to bill a language author with. Reading it from the configuration is
exactly what the principle asks for.

**Proportionality — no, and this is the finding.** Ask what actually consumes
each group, and most have no consumer yet:

| Group | What reads it | Needed for v1? |
|---|---|---|
| `truthy` | short-circuit lowering, constant folding of conditions | **yes** |
| `number` | constant folding only | no |
| `compare` | constant folding only | no |

`number` and `compare` change no emitted instruction unless the compiler folds
constants — and nothing in this proposal folds constants. Specifying them now
means specifying against imagination, in a file format, where a wrong guess is
expensive to withdraw.

#### The recommendation

**Adopt the container, populate one group.** Ship `semantics { .. }` with
`truthy` in it and nothing else. The container still earns its place with a
single occupant, because it is what makes the *next* group a group — a new block
inside an existing section rather than a new top-level keyword and a format
version bump. That is the entire argument for it, and it does not depend on how
many groups exist today.

### 3.7 A decision is cheaper than a knob

Indexing is 0-based. Not configurable, not bridged, not declared — mandated.

That is worth stating as a principle rather than a fact about arrays, because
it is the third time in this document that removing a choice beat supporting
one:

| | Instead of | Do |
|---|---|---|
| §8.3 | config identity, ranges, compatibility matrix | a version number and a build-time diagnostic |
| §7.2 | a format describing many machines | one machine, extended |
| §3.7 | `index.base` with bridging in the compiler | 0-based, everywhere |

Each knob looks free when added and is not. `index.base` alone would have cost
a semantics group, a match/bridge taxonomy carried per group forever, an
adjustment pass at every index site, and a class of bug — off-by-one in
generated code — that is miserable to debug precisely because nobody wrote the
`+ 1` by hand.

A mandate costs one line in the published contract. In this case it costs less
than that: BASIC has defaulted to 0-based for decades — `DIM A(10)` is 0..10 in
GW-BASIC and QBasic, VB6 defaults to 0 with `Option Base 1` as the opt-in, and
VB.NET and FreeBASIC are 0-based outright. 1-based is Dartmouth-era dialects and
the Fortran/MATLAB/Lua lineage, not BASIC as anyone has written it in a long
time. There is no audience to surprise.

The test for the next such question: *is this a knob because two answers are
genuinely right, or because deciding felt presumptuous?* Only the first earns
configuration.

Applied to what is left, **exactly one knob survives, and it is not one of the
ones that looked like knobs.**

Threading looked like the exception until §7.4 separated the two things called
sharing: the VM assumes shared data and mandates it, while the AST's `Rc`/`Arc`
flag governs something a plugin never exposes, so it stays available and stops
being anyone else's business.

What survives is `SharedStore` (§7.4): how mutable shared slots are
synchronised. It passes on the merits — read-mostly globals and hot mutable ones
want genuinely different machinery, the right answer depends on a workload the
toolkit cannot see, and picking one would be a guess rather than a decision.
That is what the test is for. It is not a bias against configuration; it is a
demand that configuration be *earned*, and this earns it where semantics,
indexing and identity did not.

Everything else is the machine's decision, published, or a decision nobody
should have been asked to make.

---

## 4. Both machines, expressed

**`toy-stack.nht`**

```
target  toy-stack;
format  1.0;
machine stack;

literal number = Push(value);

op add = Add;
op sub = Sub;
op mul = Mul;
op div = Div;
op neg = Neg;

lazy op and_then {
    result stack;
    Dup;
    JumpIfFalse -> done;
    Pop;
    eval rhs;
  done:
}

lazy op or_else {
    result stack;
    Dup;
    JumpIfTrue -> done;
    Pop;
    eval rhs;
  done:
}
```

**`register.nht`**

```
target  register;
format  1.0;
machine register;

literal number = LoadK { dst, value };

op add     = Add     { dst, a: lhs, b: rhs };
op sub     = Sub     { dst, a: lhs, b: rhs };
op mul     = Mul     { dst, a: lhs, b: rhs };
op div     = Div     { dst, a: lhs, b: rhs };
op neg     = Neg     { dst, a: operand };
op compare = Compare { dst, op: discriminant, a: lhs, b: rhs };

lazy op and_then {
    result lhs;
    JumpIfFalse { src: lhs } -> done;
    eval rhs -> r;
    Move { dst: lhs, src: r };
  done:
}

lazy op or_else {
    result lhs;
    JumpIfTrue { src: lhs } -> done;
    eval rhs -> r;
    Move { dst: lhs, src: r };
  done:
}
```

Both are complete for operators, and the second is a transcription of code that
already exists and works.

---

## 5. What it buys

```console
$ nh build mylang.nh --target crash-basic.nht --rust src
```

- **Operator handling disappears from the front end.** Today the author writes
  `fn add(&mut self, lhs, rhs)`. Against a declared target there is nothing to
  write: `add` means `Add`, and the emission is generated.
- **Role coverage is checked at build time.** A grammar binding a role the
  target has no instruction for is an error naming both, rather than a plugin
  that fails to load later.
- **The `.nh` stays portable.** Precedence, associativity, spelling and which
  operators exist are the language's; how they execute is the machine's. The
  same grammar can target two machines by changing one flag.

---

## 6. Where this runs out

Named honestly, because the parts a design cannot do are the parts that decide
whether it is worth building.

**Register allocation leaks.** `result lhs` works for the machine in §4 because
its allocator has stack discipline — `reuse` frees operands in reverse and takes
the lowest free slot, which is what makes "the answer goes where `lhs` was"
correct and cheap. A machine with a different allocator may need something this
format cannot say. **This is the weakest point in the proposal.** The honest
options are to declare the allocation discipline in the header and support two
or three known ones, or to accept that targets must use stack discipline and say
so.

**`eval` is control flow in a data format.** It has to be — a lazy operand is
defined by *where* its code goes. But it is the crack through which a full
lowering DSL would enter, and every future request to make the format "just
slightly more expressive" should be read as an attempt to widen it.

**Statements are out of scope, for now.** Control flow, calls and scoping are a
much larger specification, and writing it before one real target exists means
specifying against imagination. v1 covers operators and literals — the part that
is pure consequence — and statement lowering stays hand-written per front end.
If that boundary holds up in practice it can move later; if it does not, better
to learn that with a small format than a large one.

**Undecided, and yours** — but read §7 and §8 first, which resolve the first two
by removing the premise rather than answering them:

1. ~~**Unknown roles — error, or a reserved extension range?**~~ **Dissolved.**
   Under §7.3 an extension is a variant of a generic `Op<X>`, so emitting one
   the machine lacks is a type error rather than a policy. Under §8.3 the
   remaining case is a build-time diagnostic naming the role and the config —
   assistance, not a gate. There is no range to reserve and no rejection policy
   to write.
2. ~~**Is a semantic group `match` or `bridge`, and who decides?**~~
   **Dissolved.** The distinction existed because §3 had two independent sources
   of truth that could disagree. Under §8.2 there is one configuration and two
   derivations of it, so there is nothing to negotiate: semantics are inherited
   by both sides, not agreed between them.

   And the one example that motivated it is gone too: indexing is mandated
   0-based (§3.7), so there is no bridgeable group left to carry the
   distinction for.

   Note what §3.6 is *not*: a reason to give interpreters a value model. Every
   group there is a compiled-shape question for the same reason truthiness is,
   and a tree-walker answers all of them in Rust, at its own cost (§3.5).
3. **File extension and location.** `.nht` here, with no strong feeling. Whether
   targets are importable the way `.nh` files are is a real question: a family
   of related machines would want to share a base, and a `semantics` section is
   exactly the thing they would share.

---

## 7. A different shape: extend one VM instead of describing many

**Everything above assumes machines are given and must be described. They do not
have to be.** The alternative is for NailHammer to ship a base VM that languages
*extend* — a core instruction set, a threading model, and the author supplies
their own commands. (It arrived in conversation as "configurable threading";
§7.4 argues the model should be assumed rather than configured, which turned out
to remove the last knob in the design.)

This is not a new direction so much as the one the code is already on.

### 7.1 The VM is already core-plus-extensions

`crates/nh-cli/src/templates/lib_compiler.rs` has extension slots in it today:

```
    JumpIfTrue { src: Reg, target: usize },
{{vm_ops}}}
...
{{vm_exec}}
```

`--with functions` fills them from `features.rs`:

```rust
const COMPILER_FN_OPS: &str = r##"
    Call { dst: Reg, base: Reg, argc: usize, key: String, shown: String },
    Return { src: Reg },
"##;
```

So a NailHammer VM is *already* a core instruction set plus opt-in additions.
The only thing wrong with it is the mechanism: the composition happens as **text
substitution at scaffold time**, producing a file the author then owns and can
never receive a fix to. Promote that from text to types and the same
design becomes a dependency instead of a copy.

### 7.2 What it collapses

The bulk of this document exists to let one format describe machines that differ.
If everyone starts from the same machine, most of that need evaporates:

| Section | Fate |
|---|---|
| §2.1 machine model (stack vs register) | **gone** — the base VM picks one |
| §2.3 result convention | **gone** — fixed by the base VM |
| §3.2–3.3 role→opcode templates | **gone for core ops** — `add` is `Add`, in code |
| §3.6 match/bridge negotiation | **gone** — core semantics are shared code |
| §3.5 `truthy` declaration | **gone for core values** — one implementation |
| §5 build-time role checking | survives, and gets easier |

The reason it collapses is worth stating plainly: **you do not need a format for
agreeing about `Add` when both sides are running the same `Add`.** Shared code
beats a shared specification, and every question in §3.6 was a consequence of
having only the latter.

### 7.3 The extension mechanism

Rust enums are not extensible, but they are generic, which is enough:

```rust
// nh-vm
pub enum Op<X> {
    LoadK  { dst: Reg, value: Value },
    Add    { dst: Reg, a: Reg, b: Reg },
    JumpIfFalse { src: Reg, target: usize },
    Await  { dst: Reg, src: Reg },
    // ... the core set
    Ext(X),
}

pub trait Extension: Sized {
    fn exec(&self, m: &mut Machine<Self>) -> Result<Flow>;
}

pub struct Machine<X: Extension> { .. }
```

A language with no custom commands instantiates `Op<Never>` and pays nothing. A
language with them writes an enum and one `exec`. Dispatch stays static, the
core stays shared, and `Ext` is a variant rather than a reserved numeric range —
so §6's "unknown roles: error or extension range?" stops being a format question
and becomes a type error, which is strictly better.

### 7.4 Threading: assume shared data, and stop calling it a knob

**Design assumption: programs will have shared data across threads.** The VM is
built for that, not configured for it.

#### Two different things are called sharing

Worth separating before deciding anything, because conflating them produces a
flag that governs the wrong half.

`nh-runtime`'s `threadsafe` feature switches `Shared<T>` between `Rc` and `Arc`,
and `Shared<T>` wraps **AST nodes** — `Shared<Expr>`, `Shared<StmtBind>`. Its
own documentation says what it is for: *"a compiler that parses on one thread
and emits on another, or a VM that shares a stored function body between
workers, cannot use `Rc` at all."* That is a question about the **parse tree**.

The assumption above is about **runtime values** — what the machine holds in a
register and hands between programs. That type does not exist yet; the current
`Machine` keeps `f64` in registers and nothing is shared. So this is not a flag
to flip, it is a constraint on a design not yet written.

The two barely interact in the compiled shape, which is the useful part:

| | AST (`Shared<T>`) | VM values |
|---|---|---|
| tree-walking interpreter | *is* the runtime representation — flag matters | n/a |
| compiled, standalone | dead after compilation | must satisfy the assumption |
| compiled, plugin | never crosses the boundary at all (§8.1) | the host's, always |

**And the compiler is not threaded.** That is a design decision, not an
accident, and it settles the AST side completely: a front end parses, emits
bytecode and exits on one thread, so `Rc` is correct unconditionally — not
merely tolerable.

It also retires both of the reasons `shared.rs` gives for wanting `Arc`, in this
shape:

> *"a compiler that parses on one thread and emits on another"* — does not
> happen; the compiler is single-threaded by design.
>
> *"a VM that shares a stored function body between workers"* — does not apply;
> a compiled function body is bytecode, not an AST.

So the `threadsafe` feature keeps exactly one constituency: **tree-walking
interpreters**, where the AST *is* what runs and may genuinely be shared. For
everything on the compiled path the flag governs nothing anyone can observe.
That is the best outcome available for a knob — it survives where it is real,
and disappears from every contract.

#### What the assumption costs, said plainly

It reverses a decision this project made deliberately. `crates/nh-runtime/src/shared.rs`
argues *"a single-threaded interpreter should not pay for atomics it never
needs"* — and mandating shared values means every VM program pays for atomics,
including the ones that never share anything.

That is the right trade here anyway, because the alternative does not work: `Rc`
is not `Send`, so a host that runs two programs over shared data cannot use it
at all. A flag would not help either, since the *host* picks it and every plugin
inherits the consequence — which is a dictate wearing a knob's clothing.

Note also who pays and who does not. The atomics land on **runtime values in the
VM**, where sharing is the premise. They do not land on the compiler, which is
single-threaded and keeps `Rc` throughout — so the cost falls on the side that
asked for it, and compile time is untouched.

Concretely, anything a program can *share* must be `Send + Sync`. That is
narrower than "every value", and the next section is about why the difference is
most of the performance.

#### Lock granularity: most values are not shared at all

The failure to avoid is a bank-wide lock — taking one guard over the whole
register file or global table because *something* in it might be written. That
serialises every program against every other for the sake of one slot, and it is
how a concurrent VM ends up slower than a single-threaded one.

The way out is to notice that "shared" is not one category. A running program
holds three kinds of value, and only one of them needs synchronising at all:

| Kind | Examples | Cost |
|---|---|---|
| **machine-local** | registers, locals, temporaries | none — one machine, one thread, no atomics ever |
| **immutable shared** | constants, string literals, compiled code, function bodies | refcount only, no lock |
| **mutable shared** | globals, shared objects | synchronised, **per slot** |

The first row is the important one, and it is free. Registers and temporaries
belong to one `Machine` on one thread and never escape it, so they should be
plain values — no `Arc`, no atomics, no lock. A design that makes `Value` itself
uniformly atomic pays on every register move to protect the small fraction of
values that are actually shared. **Synchronisation belongs to the storage
location, not to the value type.**

The second row is nearly free and covers a large share of what a host shares:
read-only-but-shared data — code, constants, interned strings — is `Arc<T>` and
nothing more. No lock is needed because there is no writer. Getting these out of
the mutable store is most of the contention problem solved before any locking
strategy is chosen.

Only the third row is a synchronisation question, and there the rule is **per
slot, never per bank.** Writing one global must not block a read of another.

#### The default, and the override

This is the first knob in this document that **passes** the test in §3.7: two
answers are genuinely right, and which one wins depends on a workload the
toolkit cannot see. A read-mostly global table and a hot mutable one want
different machinery, and neither choice is presumptuous.

So: a sensible default, overridable at that seam and no other.

```rust
pub trait SharedStore {
    fn load(&self, slot: Slot) -> Value;
    fn store(&self, slot: Slot, v: Value);
}

pub struct Machine<X: Extension, S: SharedStore = DefaultStore> { .. }
```

A generic parameter rather than a trait object, so the default costs no virtual
call, and a program that never shares anything never touches it.

#### Measured, not predicted

`crates/nh-vm/examples/bench_store.rs` measures what this section used to
assert. Apple M4 (4P + 6E), median of 5 runs × 5M ops/thread over 64 slots,
throughput in M ops/s — **ratios matter, absolute rates do not**:

| threads · 95% read | bank RwLock | per-slot RwLock | per-slot Mutex | DashMap | AtomicU64 |
|---|---|---|---|---|---|
| 1 | 176 | **374** | 189 | 131 | *538* |
| 2 | 116 | 99 | **114** | 108 | *772* |
| 4 | 49 | 100 | **158** | 138 | *1051* |
| 8 | 32 | 57 | 79 | **84** | *517* |
| 4 · one hot slot | 53 | 46 | **62** | 26 | *1744* |

*(AtomicU64 italicised: it holds numbers only, so it is a ceiling rather than a
competitor.)*

**A first attempt at this benchmark was wrong, and the way it was wrong is
worth recording.** It ran 200k ops per thread, finishing in 0.3–1.4 ms, so
thread spawn, cache warmup and frequency scaling dominated. Two runs disagreed
about whether `RwLock` or `Mutex` was faster — by 2×, in opposite directions —
and a default was changed on the strength of one of them before the variance
was noticed. The harness now runs 25× more work, discards a warmup pass, takes
a median of five, and **prints the spread**, so a row that measured nothing
looks like it measured nothing. A benchmark that hides its variance is worse
than no benchmark, because it is believed.

**Claim 1 — a bank-wide lock serialises everything: directionally right,
overstated.** Worst at four threads and beyond (2× behind per-slot), but at two
threads it *beats* per-slot `RwLock`: one `RwLock` still admits concurrent
readers, so a bank only becomes a bottleneck once writes are frequent enough to
exclude them.

**Claim 2 — per-slot `RwLock` is a poor default: wrong as stated, and the truth
is more useful.** The ranking **inverts with thread count.** `RwLock` wins by
2× at one thread, loses by 1.6× at four, and by eight a sharded `DashMap` —
worst of all at one thread — is the best of the three. There is no single right
answer among the safe general stores, which is the strongest possible argument
for `SharedStore` being a trait: this is the knob that earned its keep, and the
crossover is why.

**Claim 3 — inline small values are worth it: confirmed, emphatically, and it
is the only unambiguous result.** An `AtomicU64` per slot is 3–10× faster in
ordinary cases and **28× faster** when several threads read one hot slot —
precisely the shape a shared-globals VM meets constantly: a counter, a flag, an
accumulator.

#### What this changes

**`DefaultStore` stays `RwLockStore`**, because a language starting out runs one
program on one thread, where it is decisively fastest, and a host that knows
better can say so in one line.

**A sharded map is a real option, not a curiosity.** It is worst uncontended and
best at eight threads, and it is also the answer for globals that are dynamic,
sparse, or shared *by name* across independently loaded languages — where slots
would need coordination between plugins that have never met. `bench_store`
implements `SharedStore` over `DashMap` **from outside the crate**, through a
dev-dependency, which demonstrates the seam works and keeps `nh-vm` itself
dependency-free.

#### The hybrid, built and measured

`HybridStore` is the design the numbers argued for: an `AtomicU64` beside a
per-slot lock, holding either an `f64`'s bits or a sentinel meaning *look in the
lock*. Writes take the lock and store the tag **last**; reads load the tag first
and return a number without touching the lock at all.

It answers the question this section left open — *does a tag check eat the
atomic advantage?* — with **no**:

| threads · 95% read | RwLock | best lock | **hybrid** | *ceiling* |
|---|---|---|---|---|
| 1 | 394 | 201 | **538** | *612* |
| 4 | 89 | 158 | **949** | *1013* |
| 8 | 61 | 88 | **485** | *509* |

88–95% of the numbers-only ceiling while storing any `Value`, and 8× the best
lock at eight threads. **`DefaultStore` is now `HybridStore`.**

**Its limit, stated plainly:** on write-heavy work the gain falls to 1.2–1.7×,
because writes still take the lock. Making writes lock-free means reclaiming
heap values while readers may hold them — hazard pointers or epochs — which is
a different and much larger problem. That is the honest next target, not a
footnote.

**Correctness rests on `heavy` staying authoritative.** It is written on every
store, numeric ones included, so the atomic is purely a fast path and never a
second source of truth. That is also why this implementation does **not**
canonicalise a value whose bits collide with the sentinel, as NaN-boxing schemes
normally must: the collision costs one trip down the slow path and nothing else,
where canonicalising would silently rewrite a caller's NaN payload to buy
nothing.

**A test that proved nothing, and what fixed it.** The first version of that
check asserted only `is_nan()` on the round trip — which passes whether or not
the implementation rewrites the value, so it did not test the thing it was named
after. Removing the canonicalisation left it green. Asserting `to_bits()`
equality makes it discriminate, and it now fails against a canonicalising
implementation. An assertion weaker than its own title is worse than no test,
because the title is what gets believed.

**What is still unsettled:** what a fully lock-free store costs for values
needing reclamation, and whether a sharded map beats the hybrid once globals are
keyed by name rather than slot.

#### What does not change

`Step` needs nothing. `Done | Failed | Awaiting(reg)` already keeps the runtime
choice with whoever drives the machine, which is exactly what a host running
several programs concurrently needs — it can interleave suspended programs
without the VM knowing a scheduler exists. It is the one part of the current
design that was already built for this.

### 7.5 What still needs describing

Not machines — **extension sets**. Crash BASIC 2.x becomes "the base VM plus
BASIC's commands", and a front end targeting it needs to know what those
commands are and how to emit them. That is a far smaller artefact than §3: no
machine model, no result convention, no semantics negotiation, no lazy-role
templates. A list of extension opcodes and their operands.

Which means the target-file idea survives at perhaps a fifth of its size, and
only for the case it is genuinely needed: **a front end compiling for someone
else's extended VM.** A language building its own VM needs no file at all.

### 7.6 The cost, stated honestly

This is the part to argue with, because it is a real change in what the project
is.

**NailHammer stops being only a code generator.** Today it emits code and owns
none of it at run time; `nh-runtime` is small, vendored, and deliberately
incapable of deciding anything. A base VM is the opposite: it decides
truthiness, arithmetic, comparison and dispatch for every language built on it,
and it has to be fast, because it is now on everyone's hot path. That is a
maintenance commitment the current architecture specifically avoided.

**It appears to re-open the vendoring question.** `nh init` vendors the runtime
so a generated project needs no registry (PUBLISHING.md). A standalone language
can vendor the VM the same way — and it looks as though a *plugin* cannot,
because it and its host would be running two copies of one machine rather than
the same one.

**That turns out to be wrong. See §8.** It is only true if the plugin boundary
is Rust types; it is false if the boundary is bytes, which is what it should be
anyway.

**It narrows what a language can be.** The freedom §3.5 defends for tree-walking
interpreters is exactly what a shared VM removes from compiled ones. That is the
trade already named — performance and suspension bought with agreement — but a
base VM makes the agreement total rather than negotiated, and a language whose
values genuinely differ from the core has to extend rather than disagree, or
write its own VM after all.

**The escape hatch has to stay open.** Whatever this becomes, `nh init` must
keep producing a standalone VM that owes the base nothing, or the tool stops
being able to build the thing it was built to build.

---

## 8. Vendoring survives, because agreement is by derivation

§7.6 claimed a plugin cannot vendor the VM. That was a mistake, and correcting
it improves the design rather than patching it.

### 8.1 The boundary is bytes, not types

Two vendored copies of the VM are only a problem if the plugin and the host
**exchange Rust values**. They should not. A plugin's job is to turn source into
bytecode; the host's job is to run bytecode. Nothing has to cross the boundary
except a serialized instruction stream.

Once that is the contract:

- Rust's lack of a stable ABI stops mattering — nobody links against anybody.
- Two copies of the same VM version are indistinguishable, because neither ever
  sees the other's types.
- A plugin does not even need the execution engine. It needs the *opcode
  definitions* and a serializer. The `Machine` stays home with the host.

This was the right answer earlier in this conversation and got lost when the
base VM arrived. The base VM does not change it; it makes it easier, because
now the two sides are the same code rather than two implementations of one spec.

### 8.2 Shared configuration, not shared agreement

The stronger half: **every language for a given VM is derived from one
configuration**, rather than each independently agreeing to a description.

```
        vm.config  ──derives──►  the host's Machine + Op set
             │
             └────derives──►  each front end's emitter + Op definitions
```

That is a different relationship from §3, and a better one. In §3 a target file
*described* a machine and a front end tried to match it — two artefacts that can
drift, with the format carrying the burden of catching it. Here there is one
artefact and two derivations, so drift is not detected, it is **impossible**.

It also explains why §3.6's match/bridge distinction dissolved rather than being
solved: semantics are not negotiated between a language and a machine, they are
*inherited by both* from the same declaration.

### 8.3 The configuration is a contract to read, not a gate to pass

An earlier draft of this section proposed a config *id* — a hash in the bytecode
header, checked at load, rejecting anything that did not match. **That was
wrong, and the reason it was wrong is worth keeping**, because the instinct will
return every time this design is extended.

It was designed for an adversary that does not exist. This is not an open market
of untrusted languages competing to smuggle bad bytecode into a VM. It is a VM
owner publishing what their machine does, and a language developer reading it.
The relationship is a written contract between colleagues, and the failure mode
is somebody misreading a document — not somebody attacking a boundary.

Enforcement designed for the second problem taxes the first. The symptoms were
already visible in the draft that proposed it: hashing the whole configuration
makes every cosmetic edit a breaking change, so the hash has to cover only
wire-relevant parts, so somebody has to decide what those are and be right
forever; hosts then want version *ranges* rather than equality, so a superset
check appears, and with it partial compatibility, and with that a matrix. Every
one of those is an edge case that frustrates a developer who wanted to write a
language.

The project already has the principle that rules this out — *do not bill the
tool writer for a decision that always goes the same way.* A compatibility
matrix bills every tool writer, forever, for a case that essentially never
arises.

**What replaces it:**

```
bytecode header:
    core version   0.4
```

A version number, the way JVM bytecode and Wasm do it. Nothing else.

**And the errors stay where they are cheap.** `nh build --config crash-basic`
already reads the configuration to generate the emitter, so it can say at build
time:

```
error: this grammar binds the `pow` role, and crash-basic has no `Pow`
  --> mylang.nh:14:5
```

That is **assistance, not enforcement** — checking what you are generating
against the thing you said you were targeting. No negotiation, no version
matrix, no identity. And if something still slips through, the VM meets an
opcode it does not know and says so. `unknown opcode 47` is a perfectly good
error; it does not need a hash to prevent it.

The distinction is the whole of it: *tell the developer early, in the tool they
are already running* beats *refuse the artefact later, in the host*.

### 8.4 What this leaves

- **Vendoring stays the default everywhere.** Standalone or plugin, a project
  vendors what it needs and depends on no registry. The property PUBLISHING.md
  protects is preserved, not traded away.
- **The extension set is part of the configuration**, so a language cannot
  emit an opcode the host lacks: it would have had to derive from a different
  configuration, and the id would differ.
- **The escape hatch is unaffected.** A language that wants its own machine
  ignores the configuration entirely and writes a VM, exactly as today.

### 8.5 Open

Three of the four questions here were about computing, comparing and ranging
over a config identity. §8.3 removed the identity, and they went with it. What
is left is smaller and more concrete.

1. **What is in the configuration?** Core version, extension opcodes and their
   operands, semantics groups, threading mode. Now that nothing hashes it, the
   answer can be generous — an entry that turns out to be documentation rather
   than input costs nothing, where under an identity scheme it would have been a
   spurious incompatibility.
2. **Where does a language developer read it?** This is the question that
   matters most under §8.3, and it is a documentation question rather than a
   format one. `nh explain --config` printing the machine's opcodes, semantics
   and extensions in the same shape `nh explain` already prints an operator
   table would cover it, and would mean the published contract and the generated
   emitter cannot disagree — they come from one file.
3. **Serialization format.** Deliberately unspecified here. It needs a version
   in the header, compactness, and no dependency a vendored project cannot carry
   — which rules out rather more crates than it sounds like.
