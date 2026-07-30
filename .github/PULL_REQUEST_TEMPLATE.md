<!--
Keep this short. The commit message is where the reasoning belongs -- this is
only what a reviewer needs before reading the diff.
-->

## What this changes

## Why

<!--
What made it necessary. If it contradicts something in DESIGN.md, say so and
say what changed your mind: that is a design revision, and it is worth having
on the record rather than discovering it later from the code.
-->

## Checks

- [ ] `cargo test --workspace`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] A test that fails without this change, if it changes behaviour
- [ ] Regenerated the checked-in examples, if it changes codegen
- [ ] Taught `examples/selfhost/nh.nh` about it, if it changes `.nh` syntax
