# Security

## Reporting a vulnerability

**Use [private vulnerability reporting](https://github.com/Crash-Continuum-LLC/NailHammer/security/advisories/new).**
It goes to the maintainers and stays closed until there is a fix to describe.
Please do not open a public issue for a vulnerability — a public issue is a
disclosure, and it is one you cannot take back.

Expect an acknowledgement within a week. If a report is accepted you will be
credited in the advisory unless you would rather not be.

## What is worth reporting

NailHammer is a code generator with a command line tool, so the interesting
boundaries are narrower than they look:

- **`nh` reading a `.nh` file.** A grammar that makes the tool write outside its
  output directory, execute something, or loop forever is a bug worth reporting.
  A grammar that makes it *panic* is a bug, but an ordinary one — file an issue.
- **Generated code.** The generator emits Rust that its user then compiles. Any
  input that makes it emit code doing something the grammar did not describe is
  the most serious class of report here.
- **`nh init` scaffolding.** It writes files and vendors a runtime. A grammar or
  project name that escapes the target directory belongs in a private report.

## What is out of scope

- **Untrusted `.nh` input is not a supported threat model.** A `.nh` file is
  source code for a tool you are running deliberately, in the same category as a
  `build.rs` or a `Makefile`. If you feed the tool a grammar you did not write,
  you are running code you did not write. The boundaries above are about the
  tool misbehaving on input a reasonable author would produce, not about
  sandboxing hostile grammars.
- **Denial of service through a deliberately pathological grammar.** PEG parsers
  can be made to backtrack badly and a generator can be handed a grammar built
  to be enormous. Both are real, neither is a vulnerability in this tool.
- **Advisories against dependencies** that Dependabot already reports. Those
  arrive on their own; a duplicate report is not needed.

## Supported versions

The newest release is the supported one. Nothing is backported: the project is
pre-1.0, the interfaces are still moving, and a fix landing anywhere but the tip
would be a promise it cannot keep. See the warning at the top of the README.
