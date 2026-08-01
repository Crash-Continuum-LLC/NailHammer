//! A BASIC-flavoured language that compiles to `nh-vm` bytecode.
//!
//! # What is *not* here
//!
//! There is no operator code in this crate. Not a `fn add`, not a match on a
//! comparison discriminant, nothing. `build.rs` says `.target("nh-vm")` and
//! `src/generated/vm_operators.rs` is written for us — because against a
//! machine that owns execution, "`add` emits `Op::Add`" is a consequence rather
//! than a decision (VM-DESIGN.md §7.2).
//!
//! What *is* here is the part only this language could have supplied: how
//! statements lower, and how variables find their slots.
//!
//! # Its twin
//!
//! `examples/vm-c` is the same language in C's clothing. Its grammar
//! looks nothing like this one — word operators, line-oriented statements — and
//! it binds the **same roles**, so `AND` here and `&` there both become
//! `Op::And`. `tests/agree.rs` checks the two produce identical output.

use nh_vm::{Emit, Emitter, NoExt, Reg};

pub mod generated {
    include!("generated/mod.rs");
}
pub mod handlers;

#[derive(pest_derive::Parser)]
#[grammar = "lang.pest"]
pub struct BasicLangParser;

/// The compiler. `Out = Reg`: evaluating a node leaves its value in a register.
///
/// Everything a compiler needs — the register allocator, the slot table, jump
/// patching — comes from [`Emitter`]. Before that trait existed this struct was
/// 130 lines of it, written identically in both twins.
#[derive(Debug, Default)]
pub struct Interp {
    emit: Emit<NoExt>,
}

impl Emitter for Interp {
    type Ext = NoExt;

    fn emit_state(&mut self) -> &mut Emit<NoExt> {
        &mut self.emit
    }

    fn emit_state_ref(&self) -> &Emit<NoExt> {
        &self.emit
    }
}

impl generated::dispatch::Semantics for Interp {
    type Out = Reg;
}

// Nothing here mentions operators. `generated/vm_operators.rs` is a module the
// generator wired into `generated/mod.rs` itself, so there is no `include!` and
// no imports to guess.

// No `nh_handlers!` here. Which form it takes is not a choice -- a compiler
// targeting a VM always needs `without short_circuit` -- so the generator makes
// the call rather than asking the author to know that.

// ---------------------------------------------------------------------------
// Driving it
// ---------------------------------------------------------------------------

/// Re-exported: a compiled program is a VM concept, not a language one, so
/// both twins use the same type rather than each defining it.
pub use nh_vm::Program;

/// Source in, bytecode out. No VM is involved yet — that is the point of the
/// split, and it is what lets a plugin compile without an execution engine.
pub fn compile(source: &str) -> std::result::Result<Program<NoExt>, String> {
    let mut sources = nh_runtime::SourceMap::new();
    let file = sources.add("<input>", source);
    let mut cx = nh_runtime::Ctx::new(sources);
    let mut host = Interp::default();

    generated::eval_source(&mut host, &mut cx, file).map_err(|ds| {
        ds.iter()
            .map(|d| d.message.clone())
            .collect::<Vec<_>>()
            .join("\n")
    })?;

    Ok(host.finish())
}

/// Runs it and gives back whatever it printed.
pub fn run(p: &Program<NoExt>) -> std::result::Result<Vec<String>, String> {
    let globals = nh_vm::DefaultStore::new(p.globals);
    let mut m = nh_vm::Machine::new(p, &globals);
    match m.resume() {
        nh_vm::Step::Done => Ok(m.output),
        nh_vm::Step::Failed(e) => Err(e),
        nh_vm::Step::Awaiting(_) => Err("this language has nothing to await".into()),
    }
}
