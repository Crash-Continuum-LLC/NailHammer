# 9. Changing what you have

Every chapter so far has added something. Real work is mostly the other three
verbs — rename, remove, reorganise — and those are where a code generator can
hurt you, because the thing being regenerated sits next to the thing you wrote.

The rule NailHammer holds to: **`nh build --rust` never overwrites or deletes a
handler file.** Everything below follows from that.

## Removing an alternative

Delete `frame` from `rule stmt` and rebuild:

```console
$ nh build pebble.nh -o src/pebble.pest --rust src
ok: generated 8 file(s) in src  [0 new handler(s), 11 kept]
warning: 1 handler file(s) no longer match any grammar alternative:
  handlers/stmt_frame.rs  (implemented — contains your code)
note: pass --prune to remove the unimplemented ones
note: pass --prune --force to remove implemented ones too, but read them first
```

Three things worth reading closely.

**It is a warning, and the build still succeeds.** `handlers/mod.rs` was
regenerated without `stmt_frame`, so the file is no longer compiled. It is
inert, not broken — your project builds and runs exactly as before, minus the
statement you deleted.

**It says `(implemented — contains your code)`.** The tool looked inside. A stub
you never touched and a handler you spent an afternoon on are different things,
and it will not treat them the same.

**`--prune` removes the untouched ones only.**

```console
$ nh build pebble.nh -o src/pebble.pest --rust src --prune
warning: 1 handler file(s) no longer match any grammar alternative:
  handlers/stmt_frame.rs  (implemented — contains your code)
note: pass --prune --force to remove implemented ones too, but read them first
```

Still there. `--prune` cleaned up nothing because there was nothing safe to
clean. To delete work you have to say so twice — `--prune --force` — and the
note tells you to read the file first, because it is about to be gone.

That asymmetry is deliberate. Regenerating is something you do dozens of times a
day, often without thinking; losing an afternoon's work should not be one
keystroke away from it.

## Renaming a label

The label is the handler's identity, so renaming `-> show` to `-> emit` is a
removal and an addition at once:

```console
$ nh build pebble.nh -o src/pebble.pest --rust src
ok: generated 9 file(s) in src  [1 new handler(s), 10 kept]
warning: 1 handler file(s) no longer match any grammar alternative:
  handlers/stmt_show.rs  (implemented — contains your code)
```

You get a fresh `handlers/stmt_emit.rs` stub and your `stmt_show.rs` is left
alone. Move your code across, delete the old file yourself, and the warning
stops.

The tool could have guessed — one added, one removed, same shape, probably a
rename. It does not, because a wrong guess silently moves your code somewhere
you did not ask for. Copying a file across is a minute; finding out later that
something was renamed for you is not.

## Renaming a binding

Different machinery, because the handler still matches — only the parameter
names disagree:

```console
warning: handlers/stmt_show.rs names its parameters differently than the
         grammar binds them
  grammar:  amount
  handler:  value
help: rename the parameters to match, so the handler says what it reads
```

A warning, not an error, as [chapter 1](01-ten-minutes.md) showed: the
parameters still line up by position, so the code is correct. What is wrong is
that the handler has stopped describing itself.

## Changing a cardinality

This one *is* an error, and it happens in `cargo build` rather than `nh build`:

```nh
rule block = "{" body:stmt? "}" -> block;   // was stmt*
```

The generated dispatch now passes `Option<Value>` where your handler takes
`Vec<Value>`, and the types do not match. Nothing warns you, because nothing
needs to — a cardinality change *is* a change to what the handler receives, and
the compiler is exactly the right thing to catch it.

That is the pattern throughout. **Renames warn, because the code still works.
Shape changes fail, because it does not.**

## Splitting the grammar

A `.nh` file that has grown can be split, and imported files are plain
fragments — no `grammar` line, because there is exactly one grammar name across
the whole set:

```nh
// lex.nh — lexical rules only
skip WHITESPACE = " " | "\t" | "\r" | "\n";
token DIGIT  = @ "0".."9";
token ALPHA  = @ "a".."z" | "A".."Z";
token NUMBER = @ DIGIT+ ("." DIGIT+)?;
token IDENT  = @ (ALPHA | "_") (ALPHA | DIGIT | "_")*;
```

```nh
grammar Pebble;
import "lex.nh";

use operators::core;
...
```

Paths resolve **relative to the importing file**, not to the working directory,
so a set of grammar files can be moved together. `nh check` prints the merged
result, which is the quickest way to confirm a split changed nothing:

```console
$ nh check pebble.nh
```

`examples/common_lex.nh` is a shared fragment used this way, and
`examples/selfhost/nh.nh` is the largest grammar in the repository if you want
to see how one is organised at size.

## What this chapter is really about

Nothing here required a decision from you about *language design*. It is all
mechanics — and the reason to spend a chapter on it is that a code generator you
cannot safely change is a code generator you will stop changing. The guarantee
that makes the rest of the book usable is the boring one: **your files are
yours, and the tool will warn you before it is confused, not after it has
tidied.**

---

Next: [When programs are wrong](10-errors.md).
