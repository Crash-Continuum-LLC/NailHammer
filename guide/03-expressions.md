# 3. Expressions you do not write

A PEG cannot express left recursion, so `a - b - c` written by hand becomes a
ladder:

```
expr    = term (("+" | "-") term)*
term    = factor (("*" | "/") factor)*
factor  = unary | primary
...
```

Every tier is a rule, a node, and a handler that mostly forwards. Add
exponentiation and you insert a tier and renumber the ones around it. Get the
associativity wrong and `2 ** 3 ** 2` quietly returns 64 instead of 512.

Pebble writes none of it.

## The atom

```nh
rule atom
  = value:NUMBER        -> number
  | text:TEXT           -> text
  | name:IDENT          -> name
  | "(" inner:expr ")"  -> pass
  ;
```

`atom` is what expressions are made **of**. The operator driver takes the
sequence of atoms and operators the parser produced and folds it into a tree
using the precedence table — so the grammar stays flat and the tree comes out
shaped correctly.

Two details in there:

**`-> pass` on the parenthesised form.** A transparent alternative: no handler,
no node of its own, it evaluates to whatever its single child does. That is
exactly right for grouping — `(1 + 2)` *is* `1 + 2`. It is legal here because
the body produces exactly one node: the literals `(` and `)` produce none, and
`inner:expr` produces one.

**`-> name` has no handler for lookup yet.** It will grow a `place` marker in
[chapter 6](06-assignment.md).

Now `nh check` passes.

## What you got

```console
$ nh explain pebble.nh
preset: operators::core

  7  =                right   lazy(lhs)  -> assign
  6  ||               left    lazy(rhs)  -> or_else
  5  &&               left    lazy(rhs)  -> and_then
  4  == != <= >= < >  left     -> compare
  3  + -              left     -> add, sub
  2  * / %            left     -> mul, div, rem
  1  ! -              prefix   -> not, neg

atom: `atom`
```

That is the table your language actually has — printed from the same data the
driver folds with, so it cannot disagree with what runs.

`operators::core` is the small one: arithmetic, comparison, logical,
assignment. `c_style` adds bitwise, shifts, compound assignment and a pipe.
`none` gives you an empty table to fill yourself.

`lazy(rhs)` on `&&` and `||` is what makes them short-circuit: the right operand
arrives **unevaluated**, so the default implementation can decline to run it.
`lazy(lhs)` on `=` is why the left side of an assignment is a *location* rather
than a value — [chapter 6](06-assignment.md).

## Roles, not spellings

The right-hand column is the interesting part. `-> compare` is a **role**: the
trait method that operator binds to. Six spellings share it, and your host
implements `compare` once, taking a discriminant.

Roles are why syntax is not semantics. In `examples/vm-basic` the operator is
spelled `AND`; in `examples/vm-c` it is `&`. Both bind the role `bit_and`, so
both emit the same instruction, and the two languages produce identical
bytecode without coordinating.

It also means the table records a real decision. BASIC's `AND` is bitwise and
**does not** short-circuit; binding it to `and_then` instead would make it
short-circuit, with no other change anywhere. The role enforces which you meant.

## Changing the table

```nh
precedence override {
    right "**" above "*" -> pow;    // add exponentiation
    remove "%";                     // Pebble has no modulo
}
```

`override` adjusts the preset in place. `precedence { .. }` without `override`
replaces it entirely, which is what a language whose operators are *words*
needs — see `examples/basic-interp/basic.nh`, where `NOT` binds looser than
comparison, the opposite of C's `!`.

Pebble keeps the preset as it comes.

## What this bought

You wrote four alternatives. You got precedence, associativity, parenthesised
grouping, unary minus, and short-circuit `&&`/`||` whose defaults are already
written in terms of one function you supply — `truthy` — which is the only part
of short-circuiting that is genuinely about your language.

---

Next: [Handlers are your language](04-handlers.md).
