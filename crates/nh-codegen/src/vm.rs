//! Generating an `Operators` implementation that emits `nh_vm::Op`.
//!
//! # What this replaces
//!
//! Today a compiler-shaped project gets an `Operators` *trait* and writes the
//! body of every method — `fn add(&mut self, lhs, rhs)` and so on for each
//! operator its language has. Against a VM that owns execution, none of those
//! bodies is a decision: `add` means emit `Op::Add`, every time, in every
//! language. VM-DESIGN.md §7.2 argues that makes it a consequence rather than a
//! choice, and the standing principle says consequences are generated.
//!
//! So this generates the whole implementation. A language targeting the VM
//! writes **no operator code at all** — what stays its own is which operators
//! exist, how they are spelled, and their precedence and associativity, all of
//! which are already declared in the `.nh` table.
//!
//! # Roles the VM does not have
//!
//! A grammar can bind a role the target has no instruction for. That is not an
//! error in the grammar and not a failure of the VM — it is a mismatch worth
//! reporting *early*, in the tool the author is already running, rather than
//! late as a plugin that will not load (§8.3). [`operators_impl`] returns those
//! roles rather than emitting something that will not compile.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use nh_operators::OperatorTable;
use nh_syntax::ast::Fixity;

use crate::ident;
use crate::operators::{emitted, Emitted};

/// How a target spells one role.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// `Op::Name { dst, a, b }`
    Binary(&'static str),
    /// `Op::Name { dst, a }`
    Unary(&'static str),
    /// `Op::Compare { dst, cmp, a, b }` — one instruction, discriminant as an
    /// operand, rather than six near-identical opcodes.
    Compare,
    /// No instruction at all: the operand *is* the answer. Unary `+`.
    Identity,
    /// A lazy role, lowered to a jump around the right operand.
    ///
    /// The payload names the jump that **skips** the right operand — so `&&`
    /// skips when the left is false and `||` when it is true. Everything else
    /// about the two is identical, which is why one shape covers both.
    ShortCircuit(&'static str),
}

/// A VM's instruction set, as far as operators are concerned.
///
/// Deliberately a plain map rather than a file format. VM-DESIGN.md §7 replaced
/// "describe many machines in a file" with "extend one machine", so what is
/// needed here is the *one* mapping, in code, where it can be type-checked.
/// A file only becomes necessary for a front end compiling against somebody
/// else's extended VM (§7.5), which is not this.
pub struct Target {
    pub name: &'static str,
    pub ops: BTreeMap<&'static str, Shape>,
}

impl Target {
    /// The core set in `nh-vm`.
    ///
    /// Small on purpose: this is a prototype VM, and a grammar using the
    /// `c_style` preset will bind plenty of roles that are not here. That
    /// produces a clear report rather than a mystery, which is the behaviour
    /// being demonstrated.
    pub fn nh_vm() -> Self {
        let mut ops = BTreeMap::new();
        ops.insert("add", Shape::Binary("Add"));
        ops.insert("sub", Shape::Binary("Sub"));
        ops.insert("mul", Shape::Binary("Mul"));
        ops.insert("div", Shape::Binary("Div"));
        ops.insert("neg", Shape::Unary("Neg"));
        ops.insert("not", Shape::Unary("Not"));
        ops.insert("bit_and", Shape::Binary("And"));
        ops.insert("bit_or", Shape::Binary("Or"));
        ops.insert("bit_xor", Shape::Binary("Xor"));
        ops.insert("rem", Shape::Binary("Rem"));
        ops.insert("pow", Shape::Binary("Pow"));
        ops.insert("shl", Shape::Binary("Shl"));
        ops.insert("shr", Shape::Binary("Shr"));
        ops.insert("shift", Shape::Compare); // grouped `<< >>` in c_style
        ops.insert("bit_not", Shape::Unary("BitNot"));
        ops.insert("pos", Shape::Identity);
        // Lazy roles. Not an instruction but a *sequence with a patch point*,
        // which is why they need their own shape: short-circuiting is control
        // flow to a compiler, not an operation.
        ops.insert("and_then", Shape::ShortCircuit("JumpIfFalse"));
        ops.insert("or_else", Shape::ShortCircuit("JumpIfTrue"));
        ops.insert("compare", Shape::Compare);
        Target { name: "nh-vm", ops }
    }
}

/// Every target by name, so a caller validates a name the same way the
/// generator resolves one.
///
/// Returning `None` for an unknown name matters: silently generating nothing
/// for `--target nh-vmm` is the same silent failure this whole review is
/// about.
pub fn target_by_name(name: &str) -> Option<Target> {
    match name {
        "nh-vm" => Some(Target::nh_vm()),
        _ => None,
    }
}

/// Names a caller can offer when a target is not recognised.
pub const TARGET_NAMES: &[&str] = &["nh-vm"];

/// A role a grammar binds that the target cannot execute.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Unsupported {
    pub role: String,
    /// A spelling that binds it, so the message can point at something the
    /// author actually wrote rather than at an abstraction.
    pub spelling: String,
}

/// Generates `impl Operators for {host}`, or reports the roles that prevent it.
pub fn operators_impl(
    table: &OperatorTable,
    target: &Target,
    host: &str,
) -> Result<String, Vec<Unsupported>> {
    let ops = emitted(table);

    // Report every unsupported role **once**, named by the first spelling that
    // binds it. Keyed by role alone: a tier like `"&" | "|" -> bit_and` binds
    // one role with two spellings, and reporting it twice would turn a wide
    // tier into a wall of duplicates saying the same thing.
    let mut missing: BTreeMap<String, String> = BTreeMap::new();
    for e in &ops {
        if !target.ops.contains_key(e.role.as_str()) {
            missing
                .entry(e.role.clone())
                .or_insert_with(|| e.op.literal.clone());
        }
    }
    if !missing.is_empty() {
        return Err(missing
            .into_iter()
            .map(|(role, spelling)| Unsupported { role, spelling })
            .collect());
    }

    let mut out = String::new();
    let _ = writeln!(
        out,
        "// Generated by NailHammer. DO NOT EDIT.\n\
         //\n\
         // Every method here emits one instruction. There is nothing to fill in:\n\
         // against a VM that owns execution, `add` means `Op::Add` in every\n\
         // language, so the body is a consequence rather than a decision.\n\
         // See VM-DESIGN.md §7.2.\n\
         //\n\
         // This is a module, not a file to `include!`. It brings its own\n\
         // imports so that wiring it in is one `pub mod` line the generator\n\
         // already wrote -- nothing here needs a name the author has to guess.\n\
         //\n\
         // The `nh_handlers!` invocation is at the bottom of this file rather\n\
         // than in your crate. Which form it takes is not a choice: a compiler\n\
         // targeting a VM always needs `without short_circuit`, because the\n\
         // macro would otherwise write a `ShortCircuit` asking `truthy` -- a\n\
         // question a host whose `Out` is a register cannot answer.\n\
         \n\
         use nh_runtime::{{Ctx, Result, Shared}};\n\
         use nh_vm::{{Cmp, Op, Reg}};\n\
         \n\
         use super::ast::Expr;\n\
         use super::dispatch::{{CompareOp, Eval, Operators, ShortCircuit}};\n\
         \n\
         impl Operators for {host} {{"
    );

    let mut done = BTreeSet::new();
    for e in &ops {
        if !done.insert((e.role.clone(), e.tier.fixity)) {
            continue;
        }
        if matches!(target.ops[e.role.as_str()], Shape::ShortCircuit(_)) {
            continue; // a different trait; emitted below
        }
        emit_role(&mut out, e, &ops, target);
    }
    let _ = writeln!(out, "}}");

    // `&&` and `||` live on `ShortCircuit`, not `Operators`, because their
    // meaning depends on the *shape* of the host rather than on the language:
    // an interpreter gets them written by `nh_handlers!`, and a compiler emits
    // a jump. Only the second is generated here.
    let lazy: Vec<&Emitted<'_>> = ops
        .iter()
        .filter(|e| matches!(target.ops.get(e.role.as_str()), Some(Shape::ShortCircuit(_))))
        .collect();

    if !lazy.is_empty() {
        let _ = writeln!(out, "\nimpl ShortCircuit for {host} {{");
        let mut seen = BTreeSet::new();
        for e in &lazy {
            if !seen.insert(e.role.clone()) {
                continue;
            }
            let Some(Shape::ShortCircuit(skip)) = target.ops.get(e.role.as_str()) else {
                continue;
            };
            let m = ident(&e.role);
            let _ = writeln!(
                out,
                "\n    /// `{}` — lazy in its right operand, so it is a jump.\n\
                \x20   ///\n\
                \x20   /// The left operand is already in a register. `{skip}` skips the right\n\
                \x20   /// one, and both arms have to leave the answer in the *same* place or\n\
                \x20   /// the code after the label could not name it -- hence the `Move`.\n\
                \x20   fn {m}(&mut self, lhs: Reg, rhs: Shared<Expr>, cx: &mut Ctx) -> Result<Reg> {{\n\
                \x20       let skip = self.emit(Op::{skip} {{ src: lhs, target: usize::MAX }});\n\
                \x20       let r = rhs.eval(self, cx)?;\n\
                \x20       if r != lhs {{\n\
                \x20           self.emit(Op::Move {{ dst: lhs, src: r }});\n\
                \x20           self.free(r);\n\
                \x20       }}\n\
                \x20       self.patch_to_here(skip);\n\
                \x20       Ok(lhs)\n\
                \x20   }}",
                e.op.literal
            );
        }
        let _ = writeln!(out, "}}");
    }

    // The invocation, so it is not something to know. `without short_circuit`
    // exactly when this file provided one -- a decision that always goes the
    // same way, and therefore not the author's to make.
    let _ = writeln!(
        out,
        "\n// Wires every handler module to its trait method.\n\
         nh_handlers!({host}{});",
        if lazy.is_empty() { "" } else { ", without short_circuit" }
    );

    Ok(out)
}

fn emit_role(out: &mut String, e: &Emitted<'_>, all: &[Emitted<'_>], target: &Target) {
    let m = ident(&e.role);
    let shape = target.ops[e.role.as_str()];

    match (shape, e.tier.fixity) {
        (Shape::Unary(op), Fixity::Prefix | Fixity::Postfix) => {
            let _ = writeln!(
                out,
                "\n    /// `{}` — emits `Op::{op}`.\n\
                \x20   fn {m}(&mut self, operand: Reg) -> Result<Reg> {{\n\
                \x20       let dst = self.reuse(&[operand]);\n\
                \x20       self.emit(Op::{op} {{ dst, a: operand }});\n\
                \x20       Ok(dst)\n\
                \x20   }}",
                e.op.literal
            );
        }
        (Shape::Binary(op), Fixity::Left | Fixity::Right) => {
            let _ = writeln!(
                out,
                "\n    /// `{}` — emits `Op::{op}`.\n\
                \x20   fn {m}(&mut self, lhs: Reg, rhs: Reg) -> Result<Reg> {{\n\
                \x20       let dst = self.reuse(&[lhs, rhs]);\n\
                \x20       self.emit(Op::{op} {{ dst, a: lhs, b: rhs }});\n\
                \x20       Ok(dst)\n\
                \x20   }}",
                e.op.literal
            );
        }
        (Shape::Compare, Fixity::Left | Fixity::Right) => {
            // The discriminant's variants are named after the *spellings* the
            // grammar used -- `==` becomes `EqEq`, `<>` becomes `LtGt` -- so
            // this cannot be a fixed list. It is built from the tier, which is
            // what lets the C twin's `==` and the BASIC twin's `=` both reach
            // `Cmp::Eq` from differently-named variants.
            let mut arms = String::new();
            for o in all.iter().filter(|o| o.role == e.role) {
                let Some(variant) = &o.variant else { continue };
                let cmp = match o.op.literal.as_str() {
                    "==" | "=" => "Eq",
                    "!=" | "<>" | "><" => "Ne",
                    "<" => "Lt",
                    "<=" | "=<" => "Le",
                    ">" => "Gt",
                    ">=" | "=>" => "Ge",
                    // Unreachable in practice: `operators_impl` rejects a
                    // comparison spelling the machine has no ordering for
                    // before any of this runs.
                    _ => continue,
                };
                let _ = writeln!(arms, "            CompareOp::{variant} => Cmp::{cmp},");
            }
            let _ = writeln!(
                out,
                "\n    /// A whole comparison tier — one instruction, discriminant as an operand.\n\
                \x20   ///\n\
                \x20   /// The discriminant sits **between** the operands, because that is where\n\
                \x20   /// the generated trait puts it.\n\
                \x20   fn {m}(&mut self, lhs: Reg, op: CompareOp, rhs: Reg) -> Result<Reg> {{\n\
                \x20       let cmp = match op {{\n{arms}\x20       }};\n\
                \x20       let dst = self.reuse(&[lhs, rhs]);\n\
                \x20       self.emit(Op::Compare {{ dst, cmp, a: lhs, b: rhs }});\n\
                \x20       Ok(dst)\n\
                \x20   }}"
            );
        }
        // Unary `+`: the operand is the answer, so there is nothing to emit.
        // Generating a `Move` would be an instruction that does nothing, in
        // every program, forever.
        (Shape::Identity, Fixity::Prefix | Fixity::Postfix) => {
            let _ = writeln!(
                out,
                "\n    /// `{}` — the operand is the answer; no instruction is emitted.\n\
                \x20   fn {m}(&mut self, operand: Reg) -> Result<Reg> {{\n\
                \x20       Ok(operand)\n\
                \x20   }}",
                e.op.literal
            );
        }

        // A shape that does not fit the fixity it was bound at. `nh-operators`
        // already rejects one role at two fixities, so reaching here means the
        // target table disagrees with the grammar about what kind of thing a
        // role is — a bug in the target, not in the grammar.
        (shape, fixity) => {
            let _ = writeln!(
                out,
                "\n    // unrepresentable: role `{}` is {fixity:?} but the target maps it as {shape:?}",
                e.role
            );
        }
    }
}
