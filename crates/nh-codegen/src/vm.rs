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

use crate::{ident, type_name};
use crate::operators::{emitted, Emitted};

/// How a target spells one role.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Shape {
    /// `Op::Name { dst, a, b }`
    Binary(String),
    /// `Op::Name { dst, <field> }` — the operand's field name travels with the
    /// shape, because it is not always `a`: `Await` calls it `src`, since what
    /// it holds is the thing being waited on rather than an arithmetic operand.
    Unary(String, String),
    /// `Op::Ext(Path { dst, a, b })` — a **language's own** binary instruction.
    ///
    /// The reason a target is not just a table of core opcodes. A front end
    /// compiling for somebody else's extended VM binds its operators to that
    /// VM's extension set, and without this it could only reach the base
    /// instructions — which is the half of the machine nobody extended.
    ///
    /// The path is written as the generated code will spell it, e.g.
    /// `crash_vm::BasicOp::Deref`.
    ExtBinary(String),
    /// `Op::Ext(Path { dst, <field> })` — the unary form.
    ExtUnary(String, String),
    /// `Op::Compare { dst, cmp, a, b }` — one instruction, discriminant as an
    /// operand, rather than six near-identical opcodes.
    Compare,
    /// No instruction at all: the operand *is* the answer. Unary `+`.
    Identity,
    /// Sequencing: evaluate both, yield the right one.
    ///
    /// Also no instruction — the operands emitted themselves, in order, before
    /// this ran. All that is left is to say which register holds the answer and
    /// to release the other. `a, b` is exactly that.
    Sequence,
    /// Assignment: lazy in its **left** operand, which arrives as a `Place`.
    ///
    /// Different in shape from everything else here. The others take values and
    /// produce one; this takes a *target* and stores into it, so it is
    /// generated from the grammar's `place`-marked alternatives rather than
    /// from the operator alone.
    Assign,
    /// A lazy role, lowered to a jump around the right operand.
    ///
    /// The payload names the jump that **skips** the right operand — so `&&`
    /// skips when the left is false and `||` when it is true. Everything else
    /// about the two is identical, which is why one shape covers both.
    ShortCircuit(String),
}

/// A VM's instruction set, as far as operators are concerned.
///
/// Deliberately a plain map rather than a file format. VM-DESIGN.md §7 replaced
/// "describe many machines in a file" with "extend one machine", so what is
/// needed here is the *one* mapping, in code, where it can be type-checked.
/// A file only becomes necessary for a front end compiling against somebody
/// else's extended VM (§7.5), which is not this.
#[derive(Clone, Debug)]
pub struct Target {
    pub name: String,
    pub ops: BTreeMap<String, Shape>,
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
        ops.insert("add".into(), Shape::Binary("Add".into()));
        ops.insert("sub".into(), Shape::Binary("Sub".into()));
        ops.insert("mul".into(), Shape::Binary("Mul".into()));
        ops.insert("div".into(), Shape::Binary("Div".into()));
        ops.insert("neg".into(), Shape::Unary("Neg".into(), "a".into()));
        ops.insert("not".into(), Shape::Unary("Not".into(), "a".into()));
        ops.insert("bit_and".into(), Shape::Binary("And".into()));
        ops.insert("bit_or".into(), Shape::Binary("Or".into()));
        ops.insert("bit_xor".into(), Shape::Binary("Xor".into()));
        ops.insert("idiv".into(), Shape::Binary("IDiv".into()));
        ops.insert("rem".into(), Shape::Binary("Rem".into()));
        ops.insert("pow".into(), Shape::Binary("Pow".into()));
        ops.insert("shl".into(), Shape::Binary("Shl".into()));
        ops.insert("shr".into(), Shape::Binary("Shr".into()));
        ops.insert("shift".into(), Shape::Compare); // grouped `<< >>` in c_style
        ops.insert("bit_not".into(), Shape::Unary("BitNot".into(), "a".into()));
        ops.insert("pos".into(), Shape::Identity);
        ops.insert("comma".into(), Shape::Sequence);
        ops.insert("len".into(), Shape::Unary("Len".into(), "src".into()));
        ops.insert("assign".into(), Shape::Assign);
        // Suspension is a unary operation on the machine: the operand is what
        // the program is waiting on, and the result is what it is handed back.
        // `ident` escapes the role to `r#await`, since it is a Rust keyword.
        ops.insert("await".into(), Shape::Unary("Await".into(), "src".into()));
        // Lazy roles. Not an instruction but a *sequence with a patch point*,
        // which is why they need their own shape: short-circuiting is control
        // flow to a compiler, not an operation.
        ops.insert("and_then".into(), Shape::ShortCircuit("JumpIfFalse".into()));
        ops.insert("or_else".into(), Shape::ShortCircuit("JumpIfTrue".into()));
        ops.insert("compare".into(), Shape::Compare);
        Target { name: "nh-vm".into(), ops }
    }

    /// The **Crash VM**: the base machine plus Crash's own instructions.
    ///
    /// VM-DESIGN.md §7.5: *"Crash BASIC 2.x becomes 'the base VM plus BASIC's
    /// commands', and a front end targeting it needs to know what those
    /// commands are and how to emit them."* This is that list, for the roles an
    /// operator table can bind.
    ///
    /// A language targeting this is not Crash BASIC and need not resemble it.
    /// It binds whichever of these roles it has spellings for, and gets the
    /// instruction — which is what "bring your own language" means in practice.
    pub fn crash_vm() -> Self {
        let mut t = Target::nh_vm();
        t.name = "crash-vm".into();
        // `&s` — dereference a string as a variable or function name. No
        // machine has an instruction for this, which is exactly why it is an
        // extension rather than a core opcode.
        t.ops.insert("deref".into(), Shape::ExtUnary("crash_vm::BasicOp::Deref".into(), "src".into()));
        t
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
        "crash-vm" => Some(Target::crash_vm()),
        _ => None,
    }
}

/// Reads a target definition.
///
/// # Why a file
///
/// VM-DESIGN.md §7.5 reserves exactly one case for this: *"a front end
/// compiling for someone else's extended VM. A language building its own VM
/// needs no file at all."* Extending the machine and targeting it must not
/// require patching this toolkit, or "bring your own language" means "bring
/// your own language and a fork of nh".
///
/// # The format
///
/// ```text
/// // Anything after `//` is a comment.
/// target crash-vm;
/// extends nh-vm;
///
/// op deref = ext crash_vm::BasicOp::Deref { dst, src };
/// op mid   = ext crash_vm::BasicOp::Mid { dst, a, b };
/// op add   = Add { dst, a, b };
/// ```
///
/// `extends` inherits a built-in set — the base machine is not something an
/// extended VM should have to restate. `ext` marks an instruction reached
/// through `Op::Ext`, which is the whole difference between a core opcode and
/// somebody's own.
///
/// The fields decide the arity: `{ dst, a, b }` is binary, `{ dst, <name> }` is
/// unary and the name travels with it, because it is not always `a` — `Await`
/// calls it `src`, since what it holds is the thing being waited on.
///
/// Deliberately not expressible: `Compare`, `Assign` and the short-circuit
/// roles. Those are not one instruction but a shape the generator knows how to
/// build, so they come from `extends` or not at all.
pub fn parse_target(text: &str) -> Result<Target, String> {
    let mut name: Option<String> = None;
    let mut ops: BTreeMap<String, Shape> = BTreeMap::new();

    for (i, raw) in text.lines().enumerate() {
        let line = match raw.find("//") {
            Some(at) => &raw[..at],
            None => raw,
        }
        .trim();
        if line.is_empty() {
            continue;
        }
        let at = |m: &str| format!("line {}: {m}", i + 1);
        let body = line
            .strip_suffix(';')
            .ok_or_else(|| at("every declaration ends with `;`"))?
            .trim();

        if let Some(rest) = body.strip_prefix("target ") {
            name = Some(rest.trim().to_string());
        } else if let Some(rest) = body.strip_prefix("extends ") {
            let base = rest.trim();
            let inherited = target_by_name(base)
                .ok_or_else(|| at(&format!("`{base}` is not a known target to extend")))?;
            // Inherited first, so a file may override one and keep the rest.
            for (k, v) in inherited.ops {
                ops.entry(k).or_insert(v);
            }
        } else if let Some(rest) = body.strip_prefix("op ") {
            let (role, spec) = rest
                .split_once('=')
                .ok_or_else(|| at("an `op` needs `= <instruction>`"))?;
            let role = role.trim();
            let spec = spec.trim();

            let (is_ext, spec) = match spec.strip_prefix("ext ") {
                Some(r) => (true, r.trim()),
                None => (false, spec),
            };

            let (ctor, fields) = spec
                .split_once('{')
                .ok_or_else(|| at("an instruction needs `{ dst, .. }`"))?;
            let ctor = ctor.trim();
            let fields: Vec<&str> = fields
                .trim_end_matches('}')
                .split(',')
                .map(str::trim)
                .filter(|f| !f.is_empty())
                .collect();

            let Some((first, operands)) = fields.split_first() else {
                return Err(at("an instruction needs a `dst`"));
            };
            if *first != "dst" {
                return Err(at("the first field is `dst`"));
            }

            let shape = match operands {
                [f] if is_ext => Shape::ExtUnary(ctor.to_string(), f.to_string()),
                [f] => Shape::Unary(ctor.to_string(), f.to_string()),
                [a, b] if is_ext => {
                    if (*a, *b) != ("a", "b") {
                        return Err(at("a binary instruction names its operands `a, b`"));
                    }
                    Shape::ExtBinary(ctor.to_string())
                }
                [a, b] => {
                    if (*a, *b) != ("a", "b") {
                        return Err(at("a binary instruction names its operands `a, b`"));
                    }
                    Shape::Binary(ctor.to_string())
                }
                _ => return Err(at("an instruction takes one or two operands")),
            };
            ops.insert(role.to_string(), shape);
        } else {
            return Err(at("expected `target`, `extends` or `op`"));
        }
    }

    let name = name.ok_or_else(|| "no `target <name>;` declaration".to_string())?;
    Ok(Target { name, ops })
}

/// Names a caller can offer when a target is not recognised.
pub const TARGET_NAMES: &[&str] = &["nh-vm", "crash-vm"];

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
    lowered: &nh_lower::Lowered,
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

    let mut body = String::new();
    let _ = writeln!(body, "impl Operators for {host} {{");

    let mut done = BTreeSet::new();
    for e in &ops {
        if !done.insert((e.role.clone(), e.tier.fixity)) {
            continue;
        }
        match &target.ops[e.role.as_str()] {
            // A different trait, emitted below.
            Shape::ShortCircuit(_) => continue,
            // Emitted from the grammar's `place` alternatives, below, because
            // what a store looks like depends on what can be assigned to.
            Shape::Assign => continue,
            _ => {}
        }
        emit_role(&mut body, e, &ops, target);
    }

    // Assignment, generated from the `place`-marked alternatives rather than
    // from the operator, because a store depends on what is being stored *to*.
    // Inside this impl, because `assign` and `place_read` are `Operators`
    // methods -- an inherent method of the same name would compile and satisfy
    // nothing.
    if ops.iter().any(|e| matches!(target.ops.get(e.role.as_str()), Some(Shape::Assign))) {
        emit_assignment(&mut body, lowered);
    }

    let _ = writeln!(body, "}}");

    // `&&` and `||` live on `ShortCircuit`, not `Operators`, because their
    // meaning depends on the *shape* of the host rather than on the language:
    // an interpreter gets them written by `nh_handlers!`, and a compiler emits
    // a jump. Only the second is generated here.
    let lazy: Vec<&Emitted<'_>> = ops
        .iter()
        .filter(|e| matches!(target.ops.get(e.role.as_str()), Some(Shape::ShortCircuit(_))))
        .collect();

    if !lazy.is_empty() {
        let _ = writeln!(body, "\nimpl ShortCircuit for {host} {{");
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
                body,
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
        let _ = writeln!(body, "}}");
    }

    // The invocation, so it is not something to know. `without short_circuit`
    // exactly when this file provided one -- a decision that always goes the
    // same way, and therefore not the author's to make.
    let _ = writeln!(
        body,
        "\n// Wires every handler module to its trait method.\n\
         nh_handlers!({host}{});",
        if lazy.is_empty() { "" } else { ", without short_circuit" }
    );

    // Imports, chosen from what the body actually uses. Generated code goes
    // through the author's linter, and an unused import is a warning they
    // cannot act on -- which this project treats as a defect in the generator.
    let mut runtime = vec!["Result"];
    let mut dispatch = vec!["Operators"];
    // `Emitter` is always needed: every method below calls at least one of
    // its defaults. Naming it here is what turns seven mystery methods on the
    // host into one trait the author implements in six lines.
    let mut vm = vec!["Emitter", "Op", "Reg"];
    let mut extra = String::new();

    if body.contains("Cmp::") {
        vm.push("Cmp");
        dispatch.push("CompareOp");
    }
    if !lazy.is_empty() {
        runtime.extend(["Ctx", "Shared"]);
        dispatch.extend(["Eval", "ShortCircuit"]);
        extra.push_str("use super::ast::Expr;\n");
    }
    if body.contains("Place::") {
        extra.push_str("use super::place::Place;\n");
    }
    runtime.sort_unstable();
    dispatch.sort_unstable();
    vm.sort_unstable();

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
         // The `nh_handlers!` invocation is at the bottom rather than in your\n\
         // crate. Which form it takes is not a choice: a compiler targeting a\n\
         // VM always needs `without short_circuit`, because the macro would\n\
         // otherwise write a `ShortCircuit` asking `truthy` -- a question a\n\
         // host whose `Out` is a register cannot answer.\n\
         \n\
         use nh_runtime::{{{}}};\n\
         use nh_vm::{{{}}};\n\
         \n\
         use super::dispatch::{{{}}};\n\
         {extra}\n\
         {body}",
        runtime.join(", "),
        vm.join(", "),
        dispatch.join(", "),
    );

    Ok(out)
}

fn emit_role(out: &mut String, e: &Emitted<'_>, all: &[Emitted<'_>], target: &Target) {
    let m = ident(&e.role);
    let shape = &target.ops[e.role.as_str()];

    match (shape, e.tier.fixity) {
        (Shape::Unary(op, field), Fixity::Prefix | Fixity::Postfix) => {
            let _ = writeln!(
                out,
                "\n    /// `{}` — emits `Op::{op}`.\n\
                \x20   fn {m}(&mut self, operand: Reg) -> Result<Reg> {{\n\
                \x20       let dst = self.reuse(&[operand]);\n\
                \x20       self.emit(Op::{op} {{ dst, {field}: operand }});\n\
                \x20       Ok(dst)\n\
                \x20   }}",
                e.op.literal
            );
        }
        (Shape::ExtBinary(path), Fixity::Left | Fixity::Right) => {
            let _ = writeln!(
                out,
                "\n    /// `{}` — emits `Op::Ext({path})`.\n\
                \x20   fn {m}(&mut self, lhs: Reg, rhs: Reg) -> Result<Reg> {{\n\
                \x20       let dst = self.reuse(&[lhs, rhs]);\n\
                \x20       self.emit(Op::Ext({path} {{ dst, a: lhs, b: rhs }}));\n\
                \x20       Ok(dst)\n\
                \x20   }}",
                e.op.literal
            );
        }
        (Shape::ExtUnary(path, field), _) => {
            let _ = writeln!(
                out,
                "\n    /// `{}` — emits `Op::Ext({path})`.\n\
                \x20   fn {m}(&mut self, operand: Reg) -> Result<Reg> {{\n\
                \x20       let dst = self.reuse(&[operand]);\n\
                \x20       self.emit(Op::Ext({path} {{ dst, {field}: operand }}));\n\
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

        (Shape::Sequence, Fixity::Left | Fixity::Right) => {
            let _ = writeln!(
                out,
                "\n    /// `{}` — both operands already ran; the right one is the answer.\n\
                \x20   fn {m}(&mut self, lhs: Reg, rhs: Reg) -> Result<Reg> {{\n\
                \x20       self.free(lhs);\n\
                \x20       Ok(rhs)\n\
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

/// Generates `assign` and `place_read` from the grammar's `place` alternatives.
///
/// # Why this is not one instruction
///
/// Every other role here takes values and produces one. Assignment takes a
/// *target* — a [`Place`] whose variants come from the grammar, one per
/// `place`-marked alternative — so what a store lowers to depends on what the
/// language lets you assign to, not on the operator.
///
/// A variant that is a **single name** is a variable, and lowers to a store by
/// slot. A variant with an evaluated field is a subscript or a field access,
/// and needs an instruction this machine does not have — so it gets an arm that
/// says so at run time, naming the construct, rather than silently storing
/// somewhere wrong.
fn emit_assignment(out: &mut String, lowered: &nh_lower::Lowered) {
    let places: Vec<&nh_lower::LoweredAlternative> =
        lowered.alternatives.iter().filter(|a| a.place).collect();

    if places.is_empty() {
        return;
    }

    let mut store = String::new();
    let mut read = String::new();

    for alt in &places {
        let variant = type_name(&alt.pest_rule);
        // A simple variable: exactly one binding, and it names something rather
        // than evaluating to something.
        let simple = match alt.bindings.as_slice() {
            [b] => b.token.is_some() && matches!(b.cardinality, nh_lower::Cardinality::One),
            _ => false,
        };

        if simple {
            let field = ident(&alt.bindings[0].name);
            let _ = writeln!(
                store,
                "            Place::{variant} {{ {field}, .. }} => {{\n\
                 \x20               self.store_var({field}, value);\n\
                 \x20               Ok(value)\n\
                 \x20           }}"
            );
            let _ = writeln!(
                read,
                "            Place::{variant} {{ {field}, .. }} => Ok(self.read_var({field}))"
            );
        } else if let Some((seq, idx)) = indexed(alt) {
            // `a[i] = v`. The index arrives **already evaluated**, exactly once
            // — so a subscript with a side effect runs once, which is the whole
            // reason `Place` carries evaluated fields rather than nodes.
            let seq = ident(&seq);
            let idx = ident(&idx);
            let _ = writeln!(
                store,
                "            Place::{variant} {{ {seq}, {idx}, .. }} => {{\n\
                 \x20               let target = self.read_var({seq});\n\
                 \x20               self.emit(Op::SetIndex {{ seq: target, idx: {idx}, src: value }});\n\
                 \x20               self.free(target);\n\
                 \x20               Ok(value)\n\
                 \x20           }}"
            );
            let _ = writeln!(
                read,
                "            Place::{variant} {{ {seq}, {idx}, .. }} => {{\n\
                 \x20               let target = self.read_var({seq});\n\
                 \x20               let dst = self.reuse(&[target]);\n\
                 \x20               self.emit(Op::Index {{ dst, seq: target, idx: *{idx} }});\n\
                 \x20               Ok(dst)\n\
                 \x20           }}"
            );
        } else {
            let msg = format!(
                "`{}` cannot be assigned to on this machine",
                alt.source.trim()
            );
            let _ = writeln!(
                store,
                "            Place::{variant} {{ .. }} => Err(nh_runtime::Error::unsupported({msg:?}))"
            );
            let _ = writeln!(
                read,
                "            Place::{variant} {{ .. }} => Err(nh_runtime::Error::unsupported({msg:?}))"
            );
        }
    }

    let _ = writeln!(
        out,
        "\n    /// Stores `value` at `place`, and yields it — so `a = b = 1` chains.\n\
        \x20   fn assign(&mut self, place: Place<'_, Reg>, value: Reg) -> Result<Reg> {{\n\
        \x20       match place {{\n{store}\x20       }}\n\
        \x20   }}\n\
        \n\
        \x20   /// Reads the current value at `place`, for compound assignment.\n\
        \x20   fn place_read(&mut self, place: &Place<'_, Reg>) -> Result<Reg> {{\n\
        \x20       match place {{\n{read}\x20       }}\n\
        \x20   }}"
    );
}

/// A `place` alternative that looks like `a[i]`: one name and one evaluated
/// index, in that order.
///
/// Recognised structurally rather than by spelling, so a language can write it
/// `a[i]`, `a(i)` — which is what a BASIC does — or anything else, and still
/// get an indexed store.
fn indexed(alt: &nh_lower::LoweredAlternative) -> Option<(String, String)> {
    match alt.bindings.as_slice() {
        [name, index]
            if name.token.is_some()
                && index.token.is_none()
                && matches!(name.cardinality, nh_lower::Cardinality::One)
                && matches!(index.cardinality, nh_lower::Cardinality::One) =>
        {
            Some((name.name.clone(), index.name.clone()))
        }
        _ => None,
    }
}
