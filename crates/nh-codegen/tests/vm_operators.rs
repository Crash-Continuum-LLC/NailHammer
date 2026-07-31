//! Generating operator emission against a VM (VM-DESIGN.md §7.2).
//!
//! The claim under test is that a language targeting a VM writes **no operator
//! code at all**. These check the two halves of that: what is generated when
//! the VM can execute a role, and what is *reported* when it cannot.

use nh_codegen::vm::{operators_impl, Target};
use nh_syntax::SourceMap;

fn table(source: &str) -> nh_operators::OperatorTable {
    let mut sm = SourceMap::new();
    let ast = nh_syntax::parse_source(&mut sm, "<test>", source)
        .unwrap_or_else(|e| panic!("{}", e.render(&sm)));
    nh_operators::resolve(&ast, &mut sm).unwrap_or_else(|e| panic!("{}", e.render(&sm)))
}

/// The whole point: arithmetic needs no hand-written implementation.
#[test]
fn arithmetic_generates_its_own_implementation() {
    let t = table(
        "grammar T;\nprecedence {\n  left \"+\" | \"-\";\n  \
         left \"*\" | \"/\";\n  prefix \"-\";\n  atom a;\n}\n",
    );

    let src = operators_impl(&t, &Target::nh_vm(), "Compiler").expect("all roles are executable");

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
    let t = table(
        "grammar T;\nprecedence {\n  \
         left \"==\" | \"!=\" | \"<\" | \"<=\" | \">\" | \">=\" -> compare;\n  \
         atom a;\n}\n",
    );

    let src = operators_impl(&t, &Target::nh_vm(), "Compiler").expect("compare is executable");

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
    let t = table(
        "grammar T;\nprecedence {\n  left \"+\";\n  \
         right \"**\" -> pow;\n  left \"%\" -> rem;\n  atom a;\n}\n",
    );

    let missing = operators_impl(&t, &Target::nh_vm(), "Compiler")
        .expect_err("nh-vm has no Pow or Rem");

    let roles: Vec<&str> = missing.iter().map(|u| u.role.as_str()).collect();
    assert_eq!(roles, ["pow", "rem"], "sorted and deduplicated: {missing:?}");

    // Named by something the author typed.
    assert_eq!(missing[0].spelling, "**");
    assert_eq!(missing[1].spelling, "%");
}

/// The report must name a role once even when several spellings bind it, or a
/// wide tier produces a wall of duplicates.
#[test]
fn one_report_per_role_not_per_spelling() {
    let t = table(
        "grammar T;\nprecedence {\n  \
         right \"**\" | \"^^\" -> pow;\n  atom a;\n}\n",
    );

    let missing = operators_impl(&t, &Target::nh_vm(), "Compiler").expect_err("nh-vm has no Pow");
    assert_eq!(missing.len(), 1, "{missing:?}");
    assert_eq!(missing[0].role, "pow");
}

/// A grammar with no operators at all generates an empty impl rather than
/// failing — the same way `use operators::none` produces no driver.
#[test]
fn no_operators_is_not_an_error() {
    let t = table("grammar T;\nuse operators::none;\n");
    let src = operators_impl(&t, &Target::nh_vm(), "Compiler").expect("nothing to support");
    assert!(src.contains("impl generated::dispatch::Operators for Compiler {"), "{src}");
    assert!(!src.contains("fn add"), "{src}");
}
