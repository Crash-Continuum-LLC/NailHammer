# Publishing

`nh-runtime` is a dependency of every generated project, so nothing else here is
usable by another person until it is on crates.io. Everything is packaged and
metadata-complete; what remains is the upload.

Crates depend on each other, and cargo resolves a versioned path dependency
against the registry — so they publish in dependency order, waiting for each to
become available before the next:

```console
$ cargo publish -p nh-syntax
$ cargo publish -p nh-operators
$ cargo publish -p nh-analysis
$ cargo publish -p nh-lower
$ cargo publish -p nh-codegen
$ cargo publish -p nh-build
$ cargo publish -p nh-cli
$ cargo publish -p nh-runtime
```

`cargo package -p <crate> --no-verify` succeeds for a crate whose dependencies
are already published; it fails with `no matching package named ...` for one
whose are not. That failure is the expected state before the first publish, not
a defect.

## Before the first publish

- [x] Copyright holder: **Crash Continuum LLC**.
- [x] Starting version: **0.1.0** for all eight crates.
- [ ] **After the last `cargo publish` above**, set `PUBLISHED = true` in
      `crates/nh-cli/src/init.rs` and release `nh-cli` again.

      That one constant switches a scaffolded `Cargo.toml` from
      `{ version, path }` to a plain `version`. Until then the path points into
      whatever checkout built the `nh` binary, so a scaffolded project only
      builds on that machine.

      **Order matters.** Flipping it before the crates exist would make every
      scaffolded project fail to build — worse than the current limitation, not
      better.
