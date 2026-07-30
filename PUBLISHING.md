# Publishing

**Nothing needs publishing for NailHammer to be usable.** That was not true
before the runtime was vendored, so this document used to describe a required
step. It now describes an optional one.

## How people get it today

```console
$ ./install.sh
```

`install.sh` picks a route rather than asking anyone to: a prebuilt binary from
the latest release when one exists for the platform, a source build when it does
not. Both of the routes below are what it runs, and either still works by hand.

```console
$ cargo install --git https://github.com/Crash-Continuum-LLC/NailHammer nh-cli
```

Or, with no Rust toolchain at all, take a prebuilt binary from a release:

```console
$ curl -fsSL -O https://github.com/Crash-Continuum-LLC/NailHammer/releases/latest/download/nh-macos-arm64.tar.gz
```

Tagging `v*` builds those for macOS (arm64 and x86_64), Linux, and Windows, and
attaches the VS Code extension alongside them.

Both of those were harder while the repository was private: an anonymous
download answered 404, so the prebuilt route needed `gh` to supply a login, and
the source route needed `net.git-fetch-with-cli` set because cargo's built-in
git client cannot use `gh`'s credential helper. Neither applies to a public
repository. `install.sh` still falls back to `gh` if a download 404s, which is
what makes it work against a private release without being built around one.

## What a generated project needs

Nothing but pest.

```toml
[dependencies]
nh-runtime = { path = "vendor/nh-runtime" }
pest = "2.8"
pest_derive = { version = "2.8", features = ["grammar-extras"] }
```

`nh init` writes the runtime into `vendor/nh-runtime/` — a small module tree with
one dependency — and `build.rs` shells out to the `nh` binary rather than linking
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

## Cutting a release

```console
$ git tag v0.2.0 && git push origin v0.2.0
```

That rebuilds `nh` for macOS (arm64 and x86_64), Linux and Windows, repackages
the extension, and attaches all of it. Nothing needs writing — the workflow is
in `.github/workflows/release.yml`.

**Bump `workspace.package.version` first.** A tag whose name disagrees with
`nh --version` is a small thing that costs real time later, when somebody is
trying to work out which binary they have. The eight `version = "…"` pins in the
root `Cargo.toml` move together: the workspace's own, and the seven path
dependencies that must match it.

The install instructions used to name a tag explicitly, which made them only as
current as the newest release — `v0.1.0` stayed in them through completion, the
evaluation playground, `nh trace`, the register-machine compiler scaffold and the
recovery fix, long enough to be actively misleading. They now use
`/releases/latest/download/`, which GitHub resolves to the newest release, so
there is nothing left in them for a tag to make stale.
