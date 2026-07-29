# Publishing

**Nothing needs publishing for NailHammer to be usable.** That was not true
before the runtime was vendored, so this document used to describe a required
step. It now describes an optional one.

## How people get it today

```console
$ cargo install --git https://github.com/Crash-Continuum-LLC/NailHammer nh-cli
```

The repository is private, so this needs an account with access and one cargo
setting, because cargo's built-in git client cannot use `gh`'s credential
helper:

```toml
# ~/.cargo/config.toml
[net]
git-fetch-with-cli = true
```

Or, with no Rust toolchain at all, take a prebuilt binary from a release:

```console
$ gh release download v0.1.0 --repo Crash-Continuum-LLC/NailHammer --pattern '*macos-arm64*'
```

Tagging `v*` builds those for macOS (arm64 and x86_64), Linux, and Windows, and
attaches the VS Code extension alongside them.

## What a generated project needs

Nothing but pest.

```toml
[dependencies]
nh-runtime = { path = "vendor/nh-runtime" }
pest = "2.8"
pest_derive = { version = "2.8", features = ["grammar-extras"] }
```

`nh init` writes the runtime into `vendor/nh-runtime/` — 1,130 lines, one
dependency — and `build.rs` shells out to the `nh` binary rather than linking
the generator. So a scaffolded project builds with no credentials, no cargo
configuration, and no access to this repository.

The vendored copy is pinned to the `nh` that generated it. That is the right
coupling rather than a limitation: generated code and its runtime have to agree,
and a floating dependency on `main` can break a project that has not changed.
To take a newer runtime, re-run `nh init` in a scratch directory and copy
`vendor/` across.

## If you ever go public

crates.io is a **public** registry — there is no private publishing — so this
only applies if the project stops being private.

Publish in dependency order, since each crate must exist before the next can
reference it:

```
nh-runtime → nh-syntax → nh-operators → nh-lower → nh-analysis
           → nh-codegen → nh-build → nh-cli
```

Then `cargo install nh-cli` replaces the `--git` form, and the vendoring in
`nh init` could become a `--vendor` flag rather than the default. Neither is
required: vendoring keeps working, and a project that vendors needs nothing
from a registry.

A private registry (Cloudsmith, Artifactory) is the middle path if versioned
dependencies are wanted without going public. Users would need registry
credentials, which is the burden vendoring exists to remove.

## The extension has outrun the last release

`v0.1.0` carries a `.vsix` built at that tag. The extension has moved several
versions since — completion, an evaluation playground, and the `nh trace` pane it
depends on — so that asset installs something noticeably older than the source.

Two ways out, and the second is the real one:

* Build it locally: `cd editors/vscode && npx @vscode/vsce package`.
* **Cut a tag.** `git tag v0.2.0 && git push origin v0.2.0` rebuilds `nh` for
  four platforms and repackages the extension, both attached to the release. The
  workflow is already in place; nothing needs writing.

Worth doing before pointing anyone else at the repository: the install
instructions in README.md are only as current as the newest tag.
