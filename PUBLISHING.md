# Publishing

The toolkit crates are on crates.io as of 0.2.0, joined by `nh-vm` at 0.4.0.
Nothing about a *generated project* depends on that — it vendors its runtime and needs no registry — so
publishing changed how people get `nh`, not what their projects carry.

## How people get it today

```console
$ cargo install nh-cli
```

Or, with no Rust toolchain, a prebuilt binary:

```console
$ curl -fsSL https://raw.githubusercontent.com/Crash-Continuum-LLC/NailHammer/main/install.sh | bash
```

`install.sh` picks a route rather than asking anyone to: a prebuilt binary from
the latest release when one exists for the platform, `cargo install nh-cli` when
it does not. It exists for the one case the registry cannot serve — getting a
working `nh` onto a machine with no Rust at all.

Tagging `v*` builds those binaries for macOS (arm64 and x86_64), Linux, and
Windows, and attaches the VS Code extension alongside them.

Both routes were harder while the repository was private: an anonymous download
answered 404, so the prebuilt route needed `gh` to supply a login, and the
source route needed `net.git-fetch-with-cli` set because cargo's built-in git
client cannot use `gh`'s credential helper. Neither applies now, and both
workarounds are gone from `install.sh`.

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

## Publishing to crates.io

```console
$ cargo publish --workspace
```

One command. Cargo computes the dependency order itself, so the hand-derived
chain this document used to carry is no longer something anyone has to follow:

```
nh-runtime → nh-syntax → nh-operators → nh-lower → nh-analysis
           → nh-codegen → nh-build → nh-cli
nh-vm      (depends on none of them; a `--target nh-vm` language depends on it)
```

Three things that are not obvious until they bite:

**The examples must stay `publish = false`.** They are workspace members, so
`--workspace` would otherwise push `config-example` and friends to crates.io
under names nobody wants, permanently.

**Rate limits will stop you partway.** crates.io allows a burst of five *new*
crates and then one per ten minutes. The first publish took three extra windows
after the initial five, which looks like a failure and is not — the run stops
with a `429` naming the exact time to retry, and picking up where it left off is
just re-running the command.

**Nothing may reach outside its own package.** `nh-cli` embedded the runtime
with `include_str!("../../nh-runtime/src/lib.rs")`, which resolves in a checkout
and nowhere else; packaged for a registry it is a tarball with no siblings.
Every crate but `nh-cli` verified, and `nh-cli` failed to compile. The table now lives in
`nh-runtime` behind its `vendor` feature. `cargo publish --workspace --dry-run`
is what surfaces this class of problem, and it is worth running before any
release that moved files between crates.

Vendoring in `nh init` could now become a `--vendor` flag rather than the
default, since `nh-runtime` is fetchable. It has not, and the reason is in
USAGE.md: the pin is the point.

## Cutting a release

```console
$ git tag v0.2.0 && git push origin v0.2.0
```

That rebuilds `nh` for macOS (arm64 and x86_64), Linux and Windows, repackages
the extension, and attaches all of it. Nothing needs writing — the workflow is
in `.github/workflows/release.yml`.

**Bump `workspace.package.version` first.** A tag whose name disagrees with
`nh --version` is a small thing that costs real time later, when somebody is
trying to work out which binary they have. The `version = "…"` pins in the root
`Cargo.toml` move together: the workspace's own, and the path dependencies that
must match it. Counting them here would only go stale — `grep -c 'version = "'
Cargo.toml` is the check.

Since 0.2.0 a release is two artefacts, and the bump is what makes both
possible. The tag builds the binaries; `cargo publish --workspace` puts the same
version on crates.io. **A published version can never be reused** — crates.io
has no self-serve delete outside a narrow window, and yanking withdraws a
version without freeing the number. So the bump is not bookkeeping before a
release, it is the release: forgetting it fails the publish outright rather than
producing something wrong quietly.

The install instructions used to name a tag explicitly, which made them only as
current as the newest release — `v0.1.0` stayed in them through completion, the
evaluation playground, `nh trace`, the register-machine compiler scaffold and the
recovery fix, long enough to be actively misleading. They now use
`/releases/latest/download/`, which GitHub resolves to the newest release, so
there is nothing left in them for a tag to make stale.
