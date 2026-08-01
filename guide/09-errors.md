# 7. When programs are wrong

A language that stops at the first mistake is annoying. A language that stops at
the first mistake and points at the wrong place is worse.

## Recovery

One line:

```nh
recover stmt sync ";" | "}";
```

"If a `stmt` fails to parse, skip forward to the next `;` or `}` and carry on."

```pebble
show 1 + 1;
show @@@ ;
show 2 + 2;
```

```console
$ cargo run broken.pebble
2
4
error: could not parse this `stmt`
 --> broken.pebble:2:1
  |
2 | show @@@ ;
  | ^^^^^^^^^^
help: skipped to the next sync point and carried on, so errors after this one
      are real
```

Both good statements ran. The bad one is reported once, with a span, and the
help says something that matters: **errors after this one are real**. Without
recovery, one typo cascades into a page of nonsense and you learn to ignore all
of it.

The sync set is a language decision. `;` and `}` are Pebble's statement
boundaries; a line-oriented language would sync on a newline instead.

A recovered run comes back as `Err` holding the syntax errors, even though
everything evaluable was evaluated — a reported typo is not a successful run.
Whatever your handlers collected is still on your host, which is why the output
above appears at all.

## Better messages for specific mistakes

Pest's expected-set names *rules*, which are your internal names and mean
nothing to a user. `expect` replaces one:

```nh
expect "(" in atom as "opening parenthesis";
```

Now a missing `(` says "expected opening parenthesis" instead of naming a
generated rule.

## The determinism lints

The reason this project exists. Ordered choice does what it says, and what it
says is often not what you meant:

```nh
rule kw = "let" -> short | "letter" -> long;
```

```console
$ nh check pebble.nh
warning: this alternative is unreachable: an earlier one matches `let`, which
         is a prefix of `letter`
 --> pebble.nh:8:28
  |
8 | rule kw = "let" -> short | "letter" -> long;
  |                            ^^^^^^^^^^^^^^^^
help: ordered choice takes the first match, so put the longer alternative first
```

`"letter"` can never match. Nothing in the grammar text says so, and no test
would catch it unless you happened to write one for `letter`.

```console
$ nh check --lints
Determinism lints. Silence one with `allow <name> in <rule>;`

  left-recursion           a rule that can reach itself without consuming input
  nullable-repetition      a repetition whose body can match nothing
  shadow                   an earlier alternative that makes a later one unreachable
  unreachable-alternative  an alternative after one that always matches
  duplicate-binding        the same binding name twice in one sequence
  unused                   a rule or token nothing refers to
  recover-sync             a `recover` sync point that can match nothing
  silent-binding           a binding onto a rule that produces no node
```

Five of the eight are **errors**, not warnings, because each means the grammar
cannot work at all — a left-recursive rule does not terminate, and a binding
onto a silent rule has no node to attach to. The rest fire only when the tool is
*certain*: a lint that goes off on working code is one you learn to ignore,
which costs more than it saves. When you know better than the lint:

```nh
allow shadow in kw;
```

`--deny-warnings` makes the whole set fatal, which is what you want in CI.

## While you are editing

```console
$ nh trace pebble.nh --source 'show 1 + 2 * 3;'
program  → handlers/program.rs
  · SOI body:stmt* EOI -> program
  body: Vec<Self::Out>   ⟵ evaluated first, by:
    stmt_show  → handlers/stmt_show.rs
      · "show" value:expr ";" -> show
      value: Self::Out   ⟵ evaluated first, by:
        Operators::add
          · `+` — left-associative, precedence 3
          ...
```

Which handler gets what, with operators folded the way the driver folds them —
which nothing else can show you, because precedence lives in the table rather
than in the grammar. Nothing is compiled; your grammar is interpreted, so it
costs a parse.

`--json` gives the same tree as data, and `nh check --json` gives diagnostics
the same way. The VS Code extension uses both: `nh check --json` fills the
Problems panel as you type, and `nh trace` runs in a live pane.

---

Next: [Choosing a host shape](10-hosts.md).
