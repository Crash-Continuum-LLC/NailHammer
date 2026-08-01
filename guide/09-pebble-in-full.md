# Appendix: Pebble in full

The grammar the book builds, in one piece. This is the whole language
description — there is no other file that says what Pebble is.

```nh
grammar Pebble;

use operators::core;

skip WHITESPACE = " " | "\t" | "\r" | "\n";
skip COMMENT    = "#" (!"\n" ANY)*;

token DIGIT  = @ "0".."9";
token ALPHA  = @ "a".."z" | "A".."Z";
token NUMBER = @ DIGIT+ ("." DIGIT+)?;
token IDENT  = @ (ALPHA | "_") (ALPHA | DIGIT | "_")*;
token TEXT   = @ "\"" (!"\"" ANY)* "\"";

reserved from IDENT { "let" "show" "if" "else" "while" "not" }

rule program = SOI body:stmt* EOI -> program;

rule stmt
  = "let" name:IDENT "=" value:expr ";"          -> declare
  | "show" value:expr ";"                        -> show
  | "if" cond:expr lazy then:block
      lazy otherwise:else_tail?                  -> branch
  | "while" lazy cond:expr lazy body:block       -> loop
  | value:expr ";"                               -> evaluate
  ;

rule else_tail = "else" body:block -> pass;

rule block = "{" body:stmt* "}" -> block;

rule atom
  = value:NUMBER        -> number
  | text:TEXT           -> text
  | name:IDENT          -> name place
  | "(" inner:expr ")"  -> pass
  ;

recover stmt sync ";" | "}";
```

## What that generated

Ten handler files, because there are ten labelled alternatives:

```
handlers/program.rs        handlers/stmt_declare.rs   handlers/atom_number.rs
handlers/block.rs          handlers/stmt_show.rs      handlers/atom_text.rs
                           handlers/stmt_branch.rs    handlers/atom_name.rs
                           handlers/stmt_loop.rs
                           handlers/stmt_evaluate.rs
```

`else_tail` gets none — `-> pass` is transparent.

And their signatures, which are the book's argument in one place:

```rust
program        (host, body: Vec<Value>,           cx)
block          (host, body: Vec<Value>,           cx)
stmt_declare   (host, name: &str, value: Value,   cx)
stmt_show      (host, value: Value,               cx)
stmt_evaluate  (host, value: Value,               cx)
stmt_branch    (host, cond: Value,
                      then: &Shared<Block>,
                      otherwise: Option<&Shared<ElseTail>>, cx)
stmt_loop      (host, cond: &Shared<Expr>,
                      body: &Shared<Block>,       cx)
atom_number    (host, value: &str,                cx)
atom_text      (host, text: &str,                 cx)
atom_name      (host, name: &str,                 cx)
```

Every parameter name came from a binding. Every type came from a cardinality or
a `lazy`. Nothing was written twice, and nothing indexes a parse tree.

## The sample program

```pebble
# Pebble — the language built in the guide.

let width = 4;
let height = 7;
show width * height + 2;

let name = "pebble";
show "hello, " + name;

if width < height {
  show "taller than wide";
} else {
  show "wider than tall";
}

let n = 1;
let total = 0;
while n <= 5 {
  total = total + n;
  n = n + 1;
}
show total;
```

```console
$ cargo run
30
hello, pebble
taller than wide
15
```

---

Back to [the beginning](README.md).
