# Contributing

The project is pre-1.0 and the interfaces are still moving, mostly *because*
somebody used the tool and the design turned out to be wrong. That is the
intended way for this to change, so a report that starts "I tried to build X and
the API fought me" is worth more here than a patch that tidies something.

## Before a large change

**Open an issue first.** Not for process — because DESIGN.md is the argument
behind almost every interface, and a change that contradicts it is either wrong
or is a revision of the design that deserves to be written down as one. Finding
that out after the work is done wastes your afternoon, not mine.

Small fixes — a wrong error message, a broken link, a case the lints miss — need
no preamble. Send them.

## What CI enforces

Everything below runs on every pull request. Running it locally first is faster
than finding out from a red check:

```console
$ cargo test --workspace
$ cargo clippy --workspace --all-targets -- -D warnings
$ cargo test -p nh-cli -- --ignored     # scaffolds a project and builds it; slow
```

Two constraints in there are deliberate and are not negotiable style points:

- **Clippy is `-D warnings`.** Generated code is read by the user's linter, and
  a warning they cannot act on is a defect in this tool.
- **Checked-in generated output must match the generator.** If you change
  codegen, re-run the `nh build` commands in `.github/workflows/ci.yml` and
  commit the result, or CI will tell you the examples have drifted.

## Tests

A change to behaviour needs a test that fails without it. The repository's own
history is the standard to match: `tests: prove suspension where it could
actually break` exists because two passing tests were both testing the easy
case. A test that cannot fail proves nothing.

## Commit messages

Lead with the claim, then explain why underneath. `git log` is the reference —
the subject line says what changed and the body says what made it necessary,
including the evidence. "Fix bug" tells a future reader nothing they could not
see from the diff.

## Grammar changes

`.nh` is self-hosted: `examples/selfhost/nh.nh` describes the language. A change
to the syntax that does not also teach `nh.nh` about it will stop the grammar
parsing itself, and CI will catch that — which is the feature working, not an
obstacle.

## License

Contributions are under the MIT license, matching the rest of the repository.
