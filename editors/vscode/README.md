# NailHammer for VS Code

Grammar authoring for `.nh` files: highlighting, live diagnostics, scaffolding,
and tasks.

## What it does

**Syntax highlighting** that reflects what the constructs *mean*, not just where
the keywords are. A binding reads as a parameter because that is what it becomes;
`-> pass` looks different from a real label because it generates no handler;
`SOI` and `EOI` are visibly builtins, because forgetting them is this project's
most-repeated mistake.

**Live diagnostics** in the Problems panel, from `nh check --json`. You get the
lint name as the diagnostic code, the `help:` line in the hover, and — where a
diagnostic names two places, like an alternative shadowed by an earlier one —
both ends as related information you can click between.

**Scaffolding.** `NailHammer: New Project…` asks for a name and a file
extension, runs `nh init`, and offers to open the result. That project builds
and runs as-is.

**Commands and tasks** for `check`, `build`, and `explain`. `explain` opens the
resolved operator table, which is the answer to "what precedence did I actually
get".

## Setup

The extension shells out to the `nh` binary. If it is not on your `PATH`, point
the setting at a build:

```jsonc
// .vscode/settings.json
{
  "nailhammer.executable": "${workspaceFolder}/target/debug/nh"
}
```

The NailHammer repository ships that setting already.

| Setting | Default | |
|---|---|---|
| `nailhammer.executable` | `nh` | Path to the binary |
| `nailhammer.checkOnType` | `true` | Re-check while typing |
| `nailhammer.checkDelay` | `400` | Milliseconds of quiet before re-checking |
| `nailhammer.denyWarnings` | `false` | Treat determinism warnings as errors |
| `nailhammer.rustOutDir` | `src` | Where `Build` generates Rust |

## Language features

Completion is context-aware — it offers what is legal where the cursor is,
rather than one flat keyword list:

| Where | What you get |
|---|---|
| Start of a line | The declaration keywords, each with what it is for |
| `use operators::` | The four presets |
| `allow ` | The eight lint names |
| `-> ` | `pass`, plus the operator roles |
| Inside `precedence { }` | `left`/`right`/`prefix`/`postfix`, `above`/`below`, `word`, `atom` |
| `recover `, `expect … in ` | Rule names from this file |
| `reserved from `, `guard from ` | Token names from this file |
| Anywhere else | Every rule and token you have defined, plus `SOI`/`EOI`/`ANY` |

Also: **go to definition** and **hover** on any rule or token, and an **outline**
of the file's rules and tokens. All of it runs off a scan of the open document,
which keeps working while the file is mid-edit and does not parse — exactly when
completion is wanted.

## Evaluation playground

`NailHammer: Evaluation Playground` opens a pane **beside** your grammar, split
in two: a program on top, where it goes underneath. Your grammar stays where it
is.

```
                 │  playground.mylang   (edit this)
  your .nh       ├──────────────────────────────────
  (untouched)    │  where it goes       (updates as
                 │                       you type)
```

The program is an in-memory buffer — nothing is written to disk — but it is a
*named* one, so the tab reads `playground.mylang` and picks up whatever language
mode that suffix has.

Type a program in the top pane — a line of *your* language, not the grammar —
and the pane below shows where it goes, 200ms after you stop typing. If your
project has a `sample.*` beside its grammar, the playground opens already
running from it.

There is a **▶ button in the program's tab bar** when you want it run
deliberately, and `Cmd`/`Ctrl`+`Enter` does the same. A status bar item shows
whether the last run parsed.

What the right pane tells you:

```
stmt_iff  → handlers/stmt_iff.rs
  · "if" cond:expr lazy then:block lazy otherwise:else_tail? -> iff
  cond: Self::Out   ⟵ evaluated first, by:
    Operators::compare
      · `>` — left-associative, precedence 3
  then: &Shared<Block>   ⟵ lazy: the node, unevaluated
  otherwise: Option<&Shared<ElseTail>>   ⟵ absent here
```

* **which handler** gets each construct, and the file to open;
* **what it receives** — parameter names, types, and a token's actual text;
* **which arguments have not been evaluated yet.** `lazy` is the one case where
  the thing below has *not* run before the call, and it is what people get
  wrong.

Operators route to the role they bind, not to a handler — there is no
`handlers/add.rs`, `+` goes to `Operators::add` — and they are **folded the way
the driver folds them**. That is the part nothing else can show you: precedence
lives in the operator table, not the grammar, so the parse tree is flat and has
no order in it at all. Parentheses, associativity and short-circuiting all come
out right.

Nothing is compiled. It runs `nh trace`, which interprets your grammar, so the
answer arrives as fast as parsing.

## Snippets

`grammar`, `rule`, `token`, `skip`, `reserved`, `guard`, `precedence`,
`override`, `recover`, `expect`, `lazy`. The `grammar` snippet produces an
anchored entry rule and an `atom` rule, so a new grammar starts correct rather
than starting empty.

## Why no language server

`nh check --json` returns everything an editor needs for diagnostics: severity,
range, lint code, help text, and the locations of related diagnostics. The
language features above run off a document scan, because `.nh` declarations are
one line each and start with a keyword — a parser would buy nothing a scan does
not, and a scan keeps working on a file that does not currently parse.

A server earns its place when *cross-file* answers are wanted: completing rules
from an `import`ed fragment, renaming a rule everywhere it is referenced,
finding all references. None of those work today.

Checking an unsaved buffer writes a temporary copy **beside** the real file
rather than in the system temp directory, because `import` paths resolve
relative to the importing file and a copy elsewhere would fail to find them.

## Developing

```console
$ npm install
$ npm test          # tokenises real .nh source and asserts the scopes, then typechecks
```

`test/grammar.test.js` runs the same TextMate engine VS Code does and asserts 35
scopes. Syntax highlighting is otherwise unverifiable without looking at it,
which means a broken rule survives until somebody notices a keyword has changed
colour.

Press <kbd>F5</kbd> in the NailHammer repository to launch an Extension
Development Host with this extension loaded.

```console
$ npm run package   # builds a .vsix
$ code --install-extension nailhammer-*.vsix --force
```

## If nothing is coloured

Almost always the theme, not the grammar. A TextMate grammar assigns *scopes*;
the theme decides what colour each scope gets, and a theme that only names
scopes for its own language leaves every other language at the default
foreground however correct the grammar is.

Two commands cover it:

- **NailHammer: Add Syntax Colours for This Theme** writes `.nh` rules into
  `editor.tokenColorCustomizations`, scoped to the active theme. It picks a
  light or dark palette to match, keeps any customisations you already had, and
  cannot affect another language — every selector ends in `.nh`.
- **NailHammer: Remove Syntax Colours for This Theme** undoes it, which is what
  you want once the theme itself covers the standard scopes.

The scopes used here are the widely-supported ones (`keyword.control`,
`entity.name.function`, `constant.language`, and so on), so a theme with a
normal scope map needs neither command.

## If highlighting does not appear at all

Open a `.nh` file and look at the language indicator in the status bar
(bottom-right).

- **It says "Plain Text"** — the language association is not active. Run
  **Developer: Reload Window**. If that does not fix it, run
  **Developer: Show Running Extensions** and check that NailHammer is listed.
- **It says "NailHammer" but nothing is coloured** — run
  **Developer: Inspect Editor Tokens and Scopes** and put the cursor on a
  keyword. It shows the scopes being applied and which theme rule matched. If
  the scopes are there but no colour is, the theme has no rule for them.

`npm test` tokenises real `.nh` source with the same engine VS Code uses and
asserts 36 scopes, so a genuine grammar fault shows up there first.
