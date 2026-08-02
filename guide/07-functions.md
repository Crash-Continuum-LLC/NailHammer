# 7. Functions

Functions are where a small language stops being a calculator, and where three
things that have been separate so far meet: `lazy` (a body kept for later),
bindings that are *names* rather than values, and state that belongs to one call
rather than to the program.

## The grammar

Two statements and one atom:

```nh
rule stmt
  = ...
  | "fn" name:IDENT "(" (params:IDENT ","?)* ")"
      lazy body:block                            -> define
  | "return" value:expr? ";"                     -> give
  ;

rule atom
  = ...
  | name:IDENT "(" args:exprs? ")" -> call
  | name:IDENT                     -> name place
  ;

rule exprs = args:expr ("," args:expr)* -> some;
```

Four things to pull out of that.

**`params:IDENT` binds names, not expressions.** There is nothing to evaluate
about `a` in `fn f(a)`. The handler receives `&[String]`.

**`args:expr` binds expressions.** In a *call* the arguments are values. That is
why a definition and a call cannot share a rule — they look similar and mean
opposite things, and the parameter types say so:

```rust
stmt_define  params: &[String]      // names
atom_call    args:   Option<Value>  // values
```

**`lazy body:block`** keeps the body unrun so it can be stored and executed
later — as many times as it is called, or never.

**The call alternative comes before the bare name.** Both start with `IDENT`;
ordered choice tries `call` first, and it fails on anything without a `(`, so
the bare name catches the rest. Reverse them and `f(1)` parses as the variable
`f` followed by rubbish.

**`args:expr ("," args:expr)*`** is the separated-list form from
[chapter 5](05-control-flow.md): one parameter, `Vec<Value>`, head included.

## What the state has to look like

`fn` needs somewhere to keep a function, and a call needs somewhere to keep its
parameters that a *recursive* call will not overwrite:

```rust
#[derive(Clone)]
pub struct Function {
    pub params: Vec<String>,
    pub body: Shared<Block>,      // `lazy` gave us an owned node
}

pub struct Interp {
    vars: HashMap<String, Value>,           // globals
    frames: Vec<HashMap<String, Value>>,    // one per active call
    funcs: HashMap<String, Function>,
    returned: Option<Value>,
    pub output: Vec<String>,
}
```

`frames` is the whole of scoping, and it is why `fact(4)` below does not
clobber a global called `n`:

```rust
/// Innermost frame first, then globals.
pub fn get(&self, name: &str) -> Option<&Value> {
    if let Some(frame) = self.frames.last() {
        if let Some(v) = frame.get(name) {
            return Some(v);
        }
    }
    self.vars.get(name)
}
```

## Return is a signal, not a failure

`return` has to unwind out of however many nested `if`s and `while`s it is
inside. That is the same shape as an error, and the runtime gives you the
distinction:

```rust
// handlers/stmt_give.rs
pub fn run(host: &mut Interp, value: Option<Value>, cx: &mut Ctx) -> Result<Value> {
    host.stash_return(value.unwrap_or(Value::Null));
    Err(cx.signal("return"))
}
```

`cx.signal("return")` builds an `Err` that is **not** a diagnostic. Nothing
prints it, and exactly one place catches it:

```rust
let outcome = f.body.eval(self, cx);
self.frames.pop();
match outcome {
    Err(e) if e.is_signal("return") => Ok(self.returned.take().unwrap_or(Value::Null)),
    Err(e) => Err(e),
    // Falling off the end yields null, not the last value — a function that
    // means to give something back says so.
    Ok(_) => Ok(Value::Null),
}
```

Signals are matched **by name**, so `break` and `continue` are the same
mechanism with different labels and no new machinery. A `return` outside any
function is caught by nobody and reaches the top as an error — which is exactly
what it is.

## Guard the depth

```rust
if self.frames.len() >= MAX_DEPTH {
    return Err(Error::runtime(format!(
        "call nested more than {MAX_DEPTH} deep — is the recursion missing a base case?"
    )));
}
```

Without this, `fn deep(k) { return deep(k + 1); }` takes the whole process down
with a stack overflow that no handler can catch and no diagnostic can describe.
With it:

```console
error: call nested more than 256 deep — is the recursion missing a base case?
```

A missing base case is a *program* bug. Programs written in your language will
have bugs, and the response to one should be a message.

## The call

```rust
// handlers/atom_call.rs
pub fn run(host: &mut Interp, name: &str, args: Option<Value>, cx: &mut Ctx) -> Result<Value> {
    let Some(f) = host.function(name) else {
        return cx.err(format!("`{name}` is not a function"));
    };
    let args = match args {
        Some(Value::List(items)) => items,
        Some(one) => vec![one],
        None => Vec::new(),
    };
    if args.len() != f.params.len() {
        return cx.err(format!(
            "`{name}` takes {} argument(s), got {}",
            f.params.len(),
            args.len()
        ));
    }
    host.call(&f, args, cx)
}
```

Arity is checked here, by you. The grammar cannot do it — `f(1, 2)` is a
perfectly good parse whatever `f` turns out to take. Shape is the grammar's job;
meaning is yours.

## A decision worth making on purpose

Pebble looks a function up **at the moment of the call**, and runs `fn`
statements in order. So this is an error:

```pebble
show later(3);          # error: `later` is not a function
fn later(x) { return x + 100; }
```

Many languages hoist definitions so that order does not matter. Pebble does not,
and that is a choice rather than an oversight — collecting definitions in a pass
before evaluating would be a handful of lines. It is worth knowing which one you
picked, because users will find out either way.

## It works

```pebble
fn double(n) { return n * 2; }

fn fact(n) {
  if n < 2 { return 1; }
  return n * fact(n - 1);
}

show double(21);
show fact(5);

let n = 999;
show fact(4);
show n;                       # the global survived the recursion

fn noreturn(x) { let y = x; }
show noreturn(1);
```

```console
$ cargo run
42
120
24
999
null
```

---

Next: [Case, and what a name is](08-case.md).
