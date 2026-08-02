# 13. Testing your language

A language is a program that runs other programs, which makes it unusually easy
to test well: the input is text and the output is text, and everything in
between is yours.

It also makes it easy to test *badly*, in one specific way. Do not do this:

```rust
let pairs = PebbleParser::parse(Rule::program, "show 1;").unwrap();
assert_eq!(pairs.into_inner().count(), 1);       // <- no
```

That asserts on the parse tree, which is the one part of the system you did not
write and do not control. It will pass while your language is broken, and it
will need rewriting every time you touch the grammar — which is exactly the
positional coupling this whole toolkit exists to remove. Do not reintroduce it
in your tests.

## Test through the front door

One helper, and every test is a program and its output:

```rust
fn run(src: &str) -> Result<Vec<String>, String> {
    let mut sources = SourceMap::new();
    let file = sources.add("test.pebble", src.to_string());
    let mut cx = Ctx::new(sources);
    let mut interp = Interp::default();

    match generated::eval_source(&mut interp, &mut cx, file) {
        Ok(_) => Ok(interp.output.clone()),
        Err(diags) => Err(diags.iter()
            .map(|d| d.render(cx.sources()))
            .collect::<Vec<_>>()
            .join("\n")),
    }
}
```

`eval_source` is the whole pipeline — parse, report parse errors, collect what
recovery got past, build the tree, evaluate. Testing through it means your tests
exercise the same path your users do.

Then:

```rust
#[test]
fn precedence_is_not_left_to_right() {
    assert_eq!(shows("show 4 * 7 + 2;"), ["30"]);
    assert_eq!(shows("show 2 + 4 * 7;"), ["30"]);
    assert_eq!(shows("show (2 + 4) * 7;"), ["42"]);
}
```

Three cases, because one would pass by accident: `4 * 7 + 2` is 30 whether you
multiply first or not — no it isn't, but you had to check, and that is the
point. Write the case that distinguishes.

## What is worth a test

**Decisions nothing forced you to make.** Chapter 9 left three of these for
points, and each one is a test:

```rust
#[test]
fn the_origin_is_truthy() {
    assert_eq!(shows(r#"if (0, 0) { show "yes"; }"#), ["yes"]);
}
```

Nothing in the compiler required that answer. A test is where a deliberate
choice goes so that it survives someone else's refactor.

**Bugs that are invisible until they are not.** This one guards a specific
failure:

```rust
/// The bug this guards is specific: a parameter kept in one shared map instead
/// of a per-call frame reads correctly on the way down and wrongly on the way
/// back up.
#[test]
fn recursion_does_not_clobber_the_callers_variables() {
    assert_eq!(
        shows("fn fact(n) { if n < 2 { return 1; } return n * fact(n - 1); }\
               let n = 999; show fact(4); show n;"),
        ["24", "999"]
    );
}
```

`fact(4)` alone would pass with broken scoping. It is the surviving `999` that
proves the frames are real.

**That errors are errors.** A language whose failure modes are untested has
untested failure modes:

```rust
#[test]
fn arity_is_checked() {
    let e = fails("fn f(a, b) { return a; } show f(1);");
    assert!(e.contains("takes 2 argument(s), got 1"), "{e}");
}

#[test]
fn dividing_by_zero_is_reported_rather_than_returning_infinity() {
    let e = fails("show 1 / 0;");
    assert!(e.contains("division by zero"), "{e}");
}
```

**That laziness is laziness.** The only way to prove `&&` short-circuits is to
put something explosive on the right:

```rust
#[test]
fn short_circuit_does_not_evaluate_the_right_operand() {
    // `1 / 0` is an error, so if `&&` evaluated it eagerly this would fail.
    assert_eq!(shows("show 0 && (1 / 0);"), ["0"]);
}
```

A test asserting `0 && 1` is `0` proves nothing — it passes under eager
evaluation too.

## A test that found something

`runaway_recursion_is_a_diagnostic_not_a_crash` was written to confirm the depth
guard from [chapter 7](07-functions.md) worked. It did, from the command line.
Under `cargo test` the whole run **aborted**:

```
process didn't exit successfully: ... (signal: 6, SIGABRT)
```

A cargo test thread gets far less stack than `main`, and 256 nested Pebble calls
— several Rust frames each — overflowed it. The guard was real but the number
was wrong, and only the harness with the smaller stack could show that:

```rust
/// The number is bounded by the *host* stack, not by taste: each Pebble call
/// costs several Rust frames, and a spawned thread gets far less stack than
/// `main` does. 256 passed from the command line and aborted the test runner,
/// which is exactly the sort of thing a test suite is for.
const MAX_DEPTH: usize = 128;
```

That is the argument for tests in one story. It was not a wrong *idea*; it was a
wrong *number*, invisible everywhere except under a harness.

## Check that a test can fail

The habit worth building: after a test passes, break the thing it covers and
watch it go red. A test that cannot fail is worse than no test, because it looks
like coverage.

`the_origin_is_truthy` should fail if you add `Value::Point(0.0, 0.0)` to the
falsy list. `recursion_does_not_clobber_the_callers_variables` should fail if
`get` skips the frame. If either passes anyway, the test is not testing what its
name says.

## Testing the grammar itself

Two things worth wiring into CI, both cheap:

```console
$ nh check mylang.nh --deny-warnings
```

Makes every determinism lint fatal, so a shadowed alternative fails the build
rather than waiting to confuse someone.

And **checking in your generated code**, then having CI regenerate it and fail
on a diff. That is what this repository does for every worked example: if the
generator changes what it emits, the examples move with it or the build stops.

---

Next: [Choosing a host shape](14-hosts.md).
