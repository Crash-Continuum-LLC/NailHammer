//! Generating operator emission against a VM (VM-DESIGN.md §7.2).
//!
//! The claim under test is that a language targeting a VM writes **no operator
//! code at all**. These check the two halves of that: what is generated when
//! the VM can execute a role, and what is *reported* when it cannot.

use nh_codegen::vm::{operators_impl, Target};
use nh_syntax::SourceMap;

/// Wraps a `precedence` block in the smallest grammar that lowers.
///
/// Tests here are about operators, so everything else is boilerplate — and it
/// has to be *real* boilerplate: assignment is generated from the lowered
/// alternatives rather than from the operator table, so a fake atom rule that
/// satisfied a table-only test does not survive lowering.
fn compiled(precedence: &str) -> (nh_operators::OperatorTable, nh_lower::Lowered) {
    let source = format!(
        "grammar T;\nuse operators::none;\n\
         skip WS = \" \";\n\
         token DIGIT = @ \"0\"..\"9\";\ntoken NUMBER = @ DIGIT+;\n\
         token ALPHA = @ \"a\"..\"z\";\ntoken IDENT = @ ALPHA+;\n\
         {precedence}\n\
         rule program = SOI body:expr EOI -> program;\n\
         rule atom = primary;\n\
         rule primary = value:NUMBER -> num | name:IDENT -> var place;\n"
    );
    let mut sm = SourceMap::new();
    let ast = nh_syntax::parse_source(&mut sm, "<test>", &source)
        .unwrap_or_else(|e| panic!("{}", e.render(&sm)));
    let table = nh_operators::resolve(&ast, &mut sm).unwrap_or_else(|e| panic!("{}", e.render(&sm)));
    let lowered = nh_lower::lower(&ast, &table).unwrap_or_else(|e| panic!("{}", e.render(&sm)));
    (table, lowered)
}

/// The whole point: arithmetic needs no hand-written implementation.
#[test]
fn arithmetic_generates_its_own_implementation() {
    let (t, l) = compiled(
        "precedence {\n  left \"+\" | \"-\";\n  \
         left \"*\" | \"/\";\n  prefix \"-\";\n  atom atom;\n}",
    );

    let src = operators_impl(&t, &l, &Target::nh_vm(), "Compiler").expect("all roles are executable");

    // One method per role, each emitting one instruction.
    assert!(src.contains("fn add(&mut self, lhs: Reg, rhs: Reg)"), "{src}");
    assert!(src.contains("self.emit(Op::Add { dst, a: lhs, b: rhs });"), "{src}");
    assert!(src.contains("fn mul(&mut self, lhs: Reg, rhs: Reg)"), "{src}");
    assert!(src.contains("self.emit(Op::Mul { dst, a: lhs, b: rhs });"), "{src}");

    // Prefix `-` is a different role at a different arity, and must not be
    // confused with binary subtraction.
    assert!(src.contains("fn neg(&mut self, operand: Reg)"), "{src}");
    assert!(src.contains("self.emit(Op::Neg { dst, a: operand });"), "{src}");

    // Nothing is left for a person to write.
    assert!(!src.contains("todo!"), "{src}");
    assert!(!src.contains("unimplemented"), "{src}");
    assert!(!src.contains("Err(Error::unsupported"), "no stubbed-out methods: {src}");
}

/// A grouped role stays one method taking a discriminant, all the way down to
/// one instruction taking an operand. The shape survives the whole pipeline.
#[test]
fn a_comparison_tier_becomes_one_instruction() {
    let (t, l) = compiled(
        "precedence {\n  \
         left \"==\" | \"!=\" | \"<\" | \"<=\" | \">\" | \">=\" -> compare;\n  \
         atom atom;\n}",
    );

    let src = operators_impl(&t, &l, &Target::nh_vm(), "Compiler").expect("compare is executable");

    assert!(src.contains("fn compare(&mut self, lhs: Reg, op: CompareOp, rhs: Reg)"), "{src}");
    assert!(src.contains("CompareOp::Lt => Cmp::Lt,"), "{src}");
    assert!(src.contains("CompareOp::EqEq => Cmp::Eq,"), "variant named from the spelling: {src}");
    assert!(src.contains("self.emit(Op::Compare { dst, cmp, a: lhs, b: rhs });"), "{src}");

    // Six spellings, one method — not six.
    assert_eq!(src.matches("fn compare(").count(), 1, "{src}");
}

/// §8.3 — assistance, not enforcement.
///
/// A grammar binding a role the VM cannot execute gets a report naming the role
/// *and a spelling the author actually wrote*, at build time, rather than a
/// plugin that fails to load later or generated code that will not compile.
#[test]
fn a_role_the_vm_cannot_execute_is_reported_not_emitted() {
    // `arrow` is the honest remaining gap. In a C-family grammar `->` is
    // member access, and this machine has no aggregate to reach into -- so it
    // is not an oversight, it is a language feature the VM does not have.
    let (t, l) = compiled(
        "precedence {\n  left \"+\";\n  \
         left \"->\" -> arrow;\n  right \"=\" -> assign;\n  atom atom;\n}",
    );

    let missing = operators_impl(&t, &l, &Target::nh_vm(), "Compiler")
        .expect_err("nh-vm has no member access");

    let roles: Vec<&str> = missing.iter().map(|u| u.role.as_str()).collect();
    assert_eq!(roles, ["arrow"], "sorted and deduplicated: {missing:?}");
    assert_eq!(missing[0].spelling, "->", "named by something the author typed");
}

/// The report must name a role once even when several spellings bind it, or a
/// wide tier produces a wall of duplicates.
#[test]
fn one_report_per_role_not_per_spelling() {
    let (t, l) = compiled(
        "precedence {\n  \
         left \"->\" | \"=>\" -> arrow;\n  atom atom;\n}",
    );

    let missing = operators_impl(&t, &l, &Target::nh_vm(), "Compiler").expect_err("nh-vm has no member access");
    assert_eq!(missing.len(), 1, "{missing:?}");
    assert_eq!(missing[0].role, "arrow");
}

/// A grammar with no operators at all generates an empty impl rather than
/// failing — the same way `use operators::none` produces no driver.
#[test]
fn no_operators_is_not_an_error() {
    let (t, l) = compiled("precedence { atom atom; }");
    let src = operators_impl(&t, &l, &Target::nh_vm(), "Compiler").expect("nothing to support");
    assert!(src.contains("impl Operators for Compiler {"), "{src}");
    assert!(!src.contains("fn add"), "{src}");
}

/// Assignment is generated from the grammar's `place` alternatives, not from
/// the operator — because a store depends on what is being stored *to*.
#[test]
fn assignment_lowers_to_a_store_by_slot() {
    let (t, l) = compiled("precedence {\n  right \"=\" -> assign;\n  left \"+\";\natom atom;\n}");

    let src = operators_impl(&t, &l, &Target::nh_vm(), "Compiler").expect("assign is executable");

    // The variant is named after the `place`-marked alternative, so it tracks
    // the grammar rather than a fixed list.
    assert!(src.contains("Place::PrimaryVar { name, .. }"), "{src}");
    // Through `store_var`, not straight to a slot: *where* a name lives is one
    // decision, made in the `Emitter`, so assignment inside a function reaches
    // the parameter rather than a global of the same name.
    assert!(src.contains("self.store_var(name, value);"), "{src}");

    // `a = b = 1` chains, so the store yields the value.
    assert!(src.contains("Ok(value)"), "{src}");

    // Compound assignment needs to read the target first, through the same
    // seam for the same reason.
    assert!(src.contains("fn place_read"), "{src}");
    assert!(src.contains("Ok(self.read_var(name))"), "{src}");
}

// ---------------------------------------------------------------------------
// Bring your own language
// ---------------------------------------------------------------------------

/// A target is not just a table of core opcodes.
///
/// VM-DESIGN.md §7.5 reserves one case for a target definition: *"a front end
/// compiling for someone else's extended VM."* That front end binds its own
/// spellings to that VM's **extension** set — and without a shape for it, a
/// target could only reach the base instructions, which is the half of the
/// machine nobody extended.
#[test]
fn a_target_can_bind_a_role_to_an_extension_instruction() {
    let target = nh_codegen::vm::target_by_name("crash-vm").expect("a known target");

    // The base set is inherited whole: a language targeting an extended VM did
    // not stop needing arithmetic.
    assert!(target.ops.contains_key("add"), "core roles survive");
    assert!(target.ops.contains_key("idiv"));

    // And the extension is reachable by role.
    assert!(
        matches!(target.ops.get("deref"), Some(nh_codegen::vm::Shape::ExtUnary(path, _)) if path.contains("BasicOp::Deref")),
        "the extended VM's own instruction is bound to a role"
    );
}

/// `nh-vm` is the *base* machine and must stay that way — an extension belongs
/// to whoever extended it, not to everyone.
#[test]
fn the_base_target_has_no_extensions() {
    let base = nh_codegen::vm::target_by_name("nh-vm").expect("a known target");
    assert!(base.ops.contains_key("add"));
    assert!(
        !base.ops.contains_key("deref"),
        "a language on the plain machine cannot reach somebody else's instruction"
    );
}

// ---------------------------------------------------------------------------
// Target files
// ---------------------------------------------------------------------------

use nh_codegen::vm::{parse_target, Shape};

/// **The point of the whole mechanism.** Somebody extends the VM, writes a
/// file, and a front end targets it — with no change to this toolkit. §7.5
/// reserves the target file for exactly this case, and "bring your own
/// language" is otherwise "bring your own language and a fork of nh".
#[test]
fn a_target_can_be_defined_entirely_in_a_file() {
    let t = parse_target(
        "// somebody else's machine\n\
         target my-vm;\n\
         extends nh-vm;\n\
         op deref = ext my_vm::Op::Deref { dst, src };\n\
         op repeat = ext my_vm::Op::Repeat { dst, a, b };\n",
    )
    .expect("parses");

    assert_eq!(t.name, "my-vm");
    // The base machine came whole: an extended VM did not stop needing
    // arithmetic.
    assert!(matches!(t.ops.get("add"), Some(Shape::Binary(n)) if n == "Add"));
    assert!(t.ops.contains_key("compare"), "structural roles inherit too");
    // And its own instructions are reachable by role.
    assert!(matches!(t.ops.get("deref"), Some(Shape::ExtUnary(p, f)) if p == "my_vm::Op::Deref" && f == "src"));
    assert!(matches!(t.ops.get("repeat"), Some(Shape::ExtBinary(p)) if p == "my_vm::Op::Repeat"));
}

/// A file may replace an inherited spelling — the same instruction under a
/// different name is a real thing when a VM forks.
#[test]
fn a_file_overrides_what_it_inherits() {
    let t = parse_target(
        "target v;\nextends nh-vm;\nop add = ext v::Op::Plus { dst, a, b };\n",
    )
    .unwrap();
    assert!(matches!(t.ops.get("add"), Some(Shape::ExtBinary(p)) if p == "v::Op::Plus"));
}

/// A core instruction needs no `ext` — a target file describes a machine, and
/// most of one is ordinary opcodes.
#[test]
fn a_core_instruction_needs_no_ext() {
    let t = parse_target("target v;\nop add = Add { dst, a, b };\nop neg = Neg { dst, a };\n").unwrap();
    assert!(matches!(t.ops.get("add"), Some(Shape::Binary(n)) if n == "Add"));
    // The operand's field name travels with a unary, because it is not always
    // `a`: `Await` calls it `src`.
    assert!(matches!(t.ops.get("neg"), Some(Shape::Unary(n, f)) if n == "Neg" && f == "a"));
}

/// Mistakes name the line, because a target file is written by hand.
#[test]
fn a_malformed_target_says_where() {
    for (src, want) in [
        ("op add = Add { dst, a, b };\n", "no `target"),
        ("target v;\nop add = Add { dst, a, b }\n", "ends with `;`"),
        ("target v;\nop add = Add { a, b };\n", "first field is `dst`"),
        ("target v;\nop add = Add { dst, x, y };\n", "names its operands"),
        ("target v;\nextends nope;\n", "not a known target"),
        ("target v;\nwhat is this;\n", "expected `target`"),
    ] {
        let e = parse_target(src).expect_err("should be refused");
        assert!(e.contains(want), "wanted {want:?} in {e:?}");
    }
}
