[package]
name = "{{name}}"
version = "0.1.0"
edition = "2021"
build = "build.rs"

[lib]
name = "{{name}}"
path = "src/lib.rs"

[[bin]]
name = "{{name}}"
path = "src/main.rs"

[dependencies]
# Runtime support for the generated code: views, spans, diagnostics, and the
# operator driver.
nh-runtime = {{runtimedep}}

pest = "2.8"

# `grammar-extras` is NOT a default feature, and node tags (`#name = expr`) do
# not exist without it.
#
# This is the single most costly thing to get wrong: with the feature off, the
# grammar still COMPILES and parsing still succeeds — but every tag is silently
# ignored, so every generated accessor returns nothing and nothing points at the
# cause. Leave it on.
pest_derive = { version = "2.8", features = ["grammar-extras"] }

[build-dependencies]
# Regenerates the parser and the generated Rust on every `cargo build`, so a
# grammar edit cannot leave you compiling against stale views.
nh-build = {{builddep}}
