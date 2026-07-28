# Mini BASIC

`PRINT`, `LET`, `FOR`/`NEXT`, `WHILE`/`WEND`, `IF`/`THEN`, `SUB`/`CALL`,
`FUNCTION`/`RETURN`, `GOTO`, and `EXIT`/`CONTINUE`.

```console
$ cargo run -p basic-interp -- sample.bas
times table

1	1	1
1	2	2
...
sum 1..10 is	55
17 MOD 5 =	2
3 < 4 AND NOT 0 =	-1
```

## Why this example exists

**A loop is the case that `lazy` was added for.** Handler parameters normally
arrive already evaluated — that is what makes handlers two or three lines. A
loop body cannot: it has to run once per iteration, and running it *zero* times
is a legitimate outcome.

```nh
rule stmt
  = "FOR" var:IDENT "=" from:expr "TO" to:expr step:step_clause? EOL*
      lazy body:line*
    "NEXT" closing:IDENT?  -> loop
```

`lazy` changes `body` from `Vec<Value>` — already run, once, before the handler
was called — into `&[Rc<Line>]`, which the handler runs itself:

```rust
while (step > 0.0 && i <= limit) || (step < 0.0 && i >= limit) {
    host.vars.insert(name.clone(), Value::Num(i));
    for line in body {
        line.eval(host, cx)?;       // this is what runs it
    }
    i += step;
}
```

The test that proves it is `an_empty_range_never_runs_the_body`: `FOR i = 10 TO
1` must produce no output at all. Without `lazy` the body would have run before
the loop got a chance to decline.

## Subroutines, which needed the tree to be owned

`SUB` **keeps** its body and `CALL` runs it later:

```rust
// handlers/stmt_define.rs — the body is stored, not run
host.subs.insert(name.key().to_string(), body.to_vec());
```

```rust
// handlers/stmt_call.rs — and run at every call, from anywhere
for line in &body {
    line.eval(host, cx)?;
}
```

This is the construct that proved the old design was stuck. When a `lazy`
binding borrowed the parse tree it could be *run* during the handler call and
nothing else — not stored, not returned, not run later. `SUB` was inexpressible.
The AST is owned now, so a subroutine body is ordinary data on the interpreter
(DESIGN.md §9).

`CALL` also counts its own depth: recursion is possible now, so runaway
recursion is too, and it has to report rather than overflow the stack —
a stack overflow aborts the process with no diagnostic and no location.

## `GOTO`, which needed both halves

```basic
n = 3
10 PRINT "n is", n
n = n - 1
IF n > 0 THEN GOTO 10
```

A jump needs two things a fold cannot give it. The `program` handler takes its
lines `lazy` and drives them itself:

```rust
let labels = jump_table(lines, cx)?;   // reads `line.label`, runs nothing
let mut pc = 0;
while pc < lines.len() {
    match lines[pc].eval(host, cx) {
        Ok(_) => pc += 1,
        Err(e) if e.is_signal("goto") => pc = /* wherever GOTO said */,
        Err(e) => return Err(e),
    }
}
```

- **Inspection** — the jump table reads each line's number *without running the
  line*, because `Line` is a typed struct with a `label` field.
- **Unwinding** — `GOTO` is several frames below `program`, so it raises
  `Error::Signal("goto")`. `?` propagation is already the right mechanism; the
  signal is just the variant that means "not a failure". The target line number
  rides on the interpreter, since the runtime has no idea what one is.

`IF ... THEN` guards its body with `lazy`, which is what lets a backward `GOTO`
terminate rather than spin.

## Functions, which stress more than subroutines do

`SUB` is the easy half: no arguments, no result, statement position only.

```basic
FUNCTION fact(n)
  IF n <= 1 THEN RETURN 1
  RETURN n * fact(n - 1)
END FUNCTION

PRINT fact(3) + fact(2) * 2
```

Three things a `SUB` never had to get right:

- **Parameters are local.** Each call pushes a frame, so a recursive `fact`
  does not overwrite its caller's `n` — and a global `n` in the program is
  untouched. `a_parameter_does_not_leak_into_the_caller` holds that line.
- **A call is an operand.** It appears inside an expression and the operator
  driver folds it as an atom, so `fact(3) + fact(2) * 2` groups the way
  precedence says. Call is an ordinary grammar alternative rather than an
  operator — DESIGN.md §6.7's reason for keeping call out of the table.
- **`RETURN` carries a value.** `Error::Signal` has no payload by design, so
  the value rides on the interpreter, exactly as `GOTO`'s target line does.

`lazy` shows up here in a second role worth noticing:

```nh
| "FUNCTION" name:IDENT "(" lazy params:param_list? ")" EOL*
    lazy body:line*
  "END" "FUNCTION"                      -> function
```

`lazy body:line*` defers **evaluation** — the body runs at each call. `lazy
params:param_list?` defers nothing: parameter *names* are not expressions and
there is nothing to evaluate. It is how a handler asks for the node's structure
rather than its value.

## `EXIT` and `CONTINUE`, and why signals carry a name

```basic
FOR a = 1 TO 5
  WHILE k < 5
    IF a = 2 THEN EXIT FOR    REM leaves the FOR, not the WHILE
  WEND
NEXT a
```

Each statement raises `Error::Signal` named after the construct it leaves —
`"EXIT FOR"`, `"EXIT WHILE"`, `"EXIT SUB"`, and the two `CONTINUE`s. Three
things follow, and none of them needed extra machinery:

- **Nesting resolves itself.** The `WHILE` handler catches only `"EXIT WHILE"`
  and `"CONTINUE WHILE"`, so an `EXIT FOR` passes straight through to the loop
  that owns it. No depth counting.
- **Uncaught reads well.** `EXIT SUB` outside a subroutine reports `` `EXIT SUB`
  is not inside anything that handles it ``, with a location. That is why the
  label is spelled the way the language spells it.
- **A handler can stop a signal.** `CALL` refuses to let loop control cross out
  of a `SUB`: the calling loop encloses it dynamically but not lexically, and a
  jump landing somewhere the source does not show is nobody's idea of debuggable.

## What else it shows

**Case folding is in the type.** `IDENT` is declared `case-insensitive`, so
every binding to it arrives as `Ident`, not `&str`. There is no unfolded string
to look up by mistake:

```rust
host.vars.insert(target.key(), value);          // fold to store
cx.err(format!("undefined variable `{}`", name.text()))   // report as typed
```

`Total`, `total`, and `TOTAL` are one variable; the diagnostic still says what
the programmer wrote.

**A precedence table written from scratch.** BASIC shares almost nothing with C
— `=` is comparison, `<>` is inequality, `AND`/`OR`/`NOT`/`MOD` are words — so
this uses `operators::none` and declares all eight tiers. `nh explain` prints
what it resolved to:

```console
$ nh explain basic.nh
  8  OR              left     -> bit_or
  7  AND             left     -> bit_and
  6  NOT             prefix   -> not
  5  = <> <= >= < >  left     -> compare
  4  + -             left     -> add, sub
  3  MOD             left     -> rem
  2  * /             left     -> mul, div
  1  -               prefix   -> neg
```

`AND` and `OR` bind to `bit_and`/`bit_or`, which are **strict** roles — correct
for BASIC, which evaluates both sides. Binding them to `and_then`/`or_else`
instead would have made them short-circuit, with no other change. The table
records the choice; the role enforces it.

**Six spellings, one method.** The comparison tier is all `-> compare`, so the
driver passes a discriminant instead of generating six near-identical methods.

## Two things that bit, and how the grammar answers them

**Newlines are tokens, not trivia.** With `\n` skipped as whitespace, `NEXT` at
the end of one line took the identifier starting the *next* line as its closing
variable — `NEXT` followed by `Total = 0` parsed as `NEXT Total`, and the
assignment then failed with an error pointing at the `=`. BASIC is line-oriented,
and the grammar has to say so.

**There is no `recover` here, deliberately.** An error rule matches *any* text up
to its sync point, so a recovering `line` used inside `body:line*` would swallow
the `NEXT` that ends the loop, and the repetition would never terminate.
Recovery and block-structured rules do not currently compose.
[`examples/calc-interp`](../calc-interp) shows recovery on a flat statement list,
which is where it works.

## Layout

| | |
|---|---|
| `basic.nh` | The grammar and the operator table |
| `src/handlers/**` | One file per alternative. `stmt_loop.rs` is the interesting one |
| `src/lib.rs` | `Value`, `Semantics`, and the `Operators` roles this language has |
| `src/generated/**` | Generated. Regenerate with the command below |
| `sample.bas` | The program above |

```console
$ nh build basic.nh -o src/basic.pest --rust src
```

Handler files are written once and never overwritten, so re-running is safe.
