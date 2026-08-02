# 2. Tokens, and a rule that holds a program

Start Pebble from nothing:

```console
$ nh init pebble --ext pebble --interpreter
$ cd pebble
```

We take the interpreter shape because Pebble is about language design, not code
generation — handlers return values, which is the shorter road. Chapter 8 covers
the other shapes.

Open `pebble.nh` and delete everything. We build it back up.

## The header

```nh
grammar Pebble;

use operators::core;

skip WHITESPACE = " " | "\t" | "\r" | "\n";
skip COMMENT    = "#" (!"\n" ANY)*;
```

`skip` is trivia: matched anywhere between elements, never delivered to a
handler. Comments are `#` to end of line — `(!"\n" ANY)*` reads as "any
character, as long as it is not a newline".

`use operators::core` is doing a great deal of work and gets [its own
chapter](03-expressions.md).

## Tokens

```nh
token DIGIT  = @ "0".."9";
token ALPHA  = @ "a".."z" | "A".."Z";
token NUMBER = @ DIGIT+ ("." DIGIT+)?;
token IDENT  = @ (ALPHA | "_") (ALPHA | DIGIT | "_")*;
token TEXT   = @ "\"" (!"\"" ANY)* "\"";
```

The `@` means **atomic**: no trivia is skipped inside. That matters more than it
looks. Without it, `1 . 5` would lex as one number and `he llo` as one
identifier, because whitespace is skipped between elements and the elements of a
token are still elements.

`.nh` has no `digit` or `alpha` builtin, on purpose. A character class is one
line, and a language that ships them has to decide whether `alpha` means ASCII
or Unicode — a decision that belongs to you.

## Reserving keywords

```nh
reserved from IDENT { "let" "show" "if" "else" "while" "not" }
```

This guards in **both** directions:

* `let` will not match the front of `letter`. Without the guard, ordered choice
  takes `"let"`, leaves `ter`, and you get a parse error pointing at the wrong
  place.
* `let` cannot be used as a variable name.

If you want only the first — a keyword that is still usable as an identifier —
use `guard from` instead. `.nh` itself does that, because `atom` is both a
precedence keyword and an ordinary rule name.

## The entry rule

```nh
rule program = SOI body:stmt* EOI -> program;
```

Three things in one line.

**`SOI` and `EOI` anchor it.** Trivia is skipped *between* elements, never
before the first one, so without `SOI` a program that opens with a blank line or
a comment will not parse. This fails *selectively* — `1 + 1;` parses anyway,
because the expression rule starts with a repetition — so it is the kind of bug
that shows up in someone else's file, not yours.

**`body:stmt*` is a binding.** The name `body` becomes the handler's parameter
name; the `*` makes it a list.

**`-> program` is a label, and it is required here.** Try leaving it off:

```console
$ nh check pebble.nh
error: this alternative of `program` has no label, but produces 0 or more nodes
help: an alternative with no label stands in for exactly one rule's node;
      give this one a `-> label` so it gets a node of its own
```

A rule with no label has no node of its own — it *stands in for* its single
child, like `rule atom = primary;`. `stmt*` is not a single child; it is any
number of them, so there is nothing for the rule to stand in for. Two things
count as a node here that are easy to miss: **a token is a node** (so
`body:stmt EOL+` is two or more), and **a repetition is any number of them**.

## A statement, so there is something to hold

```nh
rule stmt = "show" value:expr ";" -> show;
```

Check it:

```console
$ nh check pebble.nh
error: the operator table's `atom` names `atom`, which is not defined
help: add `rule atom = ...;` for the operator driver to fold over
```

Notice what it is *not* complaining about: `expr`. There is no `rule expr` in
this file and there never will be. `use operators::core` supplies it, and what
it wants from you is the opposite end — the rule that says what an expression is
built *out of*.

That trade is worth stating plainly, because it is the one obligation a preset
puts on you: **`use operators::<preset>` installs a table ending in `atom
atom;`, so your grammar must define `rule atom`.** It applies even to a grammar
that never writes `expr` at all — the table comes from the `use`, not from being
referenced, which is why the error points at that line rather than at a rule.

That is the next chapter.

---

Next: [Expressions you do not write](03-expressions.md).
