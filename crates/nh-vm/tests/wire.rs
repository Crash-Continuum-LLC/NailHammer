//! Bytecode across a boundary (VM-DESIGN.md §8.1).
//!
//! The claim being tested is that a program can leave the process that compiled
//! it and run somewhere else — which is what makes "plugin" mean anything, and
//! what makes two vendored copies of this crate indistinguishable.

use std::collections::HashMap;

use nh_vm::{
    Cmp, ExtCx, Extension, FnDef, Flow, LocalStore, Machine, NoExt, Op, Program, Reader, Step,
    Value, Wire, WireError, FORMAT_VERSION,
};

fn sample() -> Program<NoExt> {
    let mut fns = HashMap::new();
    fns.insert("double".to_string(), FnDef { addr: 4, arity: 1, frame: 2 });
    Program {
        code: vec![
            Op::LoadK { dst: 0, value: Value::Num(21.0) },
            Op::Call { dst: 0, base: 0, argc: 1, key: "double".into(), shown: "DOUBLE".into() },
            Op::Print { src: 0 },
            Op::Halt,
            Op::Add { dst: 1, a: 0, b: 0 },
            Op::Return { src: 1 },
        ],
        fns,
        frame: 2,
        globals: 0,
    }
}

/// The property that matters: bytecode that has been through the wire runs, and
/// produces what it produced before.
#[test]
fn a_program_survives_the_round_trip_and_still_runs() {
    let before = sample();
    let bytes = before.to_bytes();
    let after = Program::<NoExt>::from_bytes(&bytes).expect("decodes");

    assert_eq!(format!("{:?}", before.code), format!("{:?}", after.code));
    assert_eq!(before.frame, after.frame);
    assert_eq!(before.globals, after.globals);
    assert_eq!(before.fns.len(), after.fns.len());

    let globals = LocalStore::new(0);
    let mut m = Machine::new(&after, &globals);
    assert!(matches!(m.resume(), Step::Done));
    assert_eq!(m.output, ["42"], "decoded bytecode runs");
}

/// Every value kind crosses intact, including the ones with payloads.
#[test]
fn values_survive_the_round_trip() {
    let p = Program::<NoExt> {
        code: vec![
            Op::LoadK { dst: 0, value: Value::Nil },
            Op::LoadK { dst: 0, value: Value::Bool(true) },
            Op::LoadK { dst: 0, value: Value::Num(-1.5) },
            Op::LoadK { dst: 0, value: Value::str("a string with ünicode") },
            Op::Compare { dst: 0, cmp: Cmp::Ge, a: 0, b: 0 },
            Op::Halt,
        ],
        frame: 1,
        ..Program::default()
    };
    let back = Program::<NoExt>::from_bytes(&p.to_bytes()).expect("decodes");
    assert_eq!(format!("{:?}", p.code), format!("{:?}", back.code));
}

/// Encoding is deterministic, so the same program produces the same bytes.
///
/// Without this a build cannot be compared against a previous one, and the
/// `fns` map's iteration order would make the output differ run to run.
#[test]
fn encoding_is_deterministic() {
    let mut fns = HashMap::new();
    for n in ["zeta", "alpha", "mu", "beta"] {
        fns.insert(n.to_string(), FnDef { addr: 0, arity: 0, frame: 1 });
    }
    let p = Program::<NoExt> { fns, frame: 1, ..Program::default() };

    let a = p.to_bytes();
    for _ in 0..8 {
        assert_eq!(a, p.to_bytes(), "same program, different bytes");
    }
}

/// §8.3 — the whole compatibility story is a version number, checked on load,
/// and a mismatch that names both sides.
#[test]
fn a_version_mismatch_is_reported_with_both_versions() {
    let mut bytes = sample().to_bytes();
    bytes[4] = 99; // the version field, little-endian

    match Program::<NoExt>::from_bytes(&bytes) {
        Err(WireError::Version { found, expected }) => {
            assert_eq!(found, 99);
            assert_eq!(expected, FORMAT_VERSION);
            let msg = WireError::Version { found, expected }.to_string();
            assert!(msg.contains("99") && msg.contains(&FORMAT_VERSION.to_string()), "{msg}");
        }
        other => panic!("expected a version error, got {other:?}"),
    }
}

#[test]
fn something_that_is_not_bytecode_is_rejected() {
    assert_eq!(
        Program::<NoExt>::from_bytes(b"not bytecode at all").unwrap_err(),
        WireError::NotBytecode
    );
    // Shorter than the magic, so the length check has to come first.
    assert_eq!(Program::<NoExt>::from_bytes(b"NH").unwrap_err(), WireError::Truncated);
}

/// A truncated stream must be an **error at every length**, never a panic. A
/// host will meet one, and it must not take the host down.
#[test]
fn truncation_is_an_error_at_every_length_not_a_panic() {
    let bytes = sample().to_bytes();
    for n in 0..bytes.len() {
        match Program::<NoExt>::from_bytes(&bytes[..n]) {
            Err(_) => {}
            Ok(_) => panic!("a prefix of {n} bytes decoded as a whole program"),
        }
    }
    // And the whole thing still works, so the loop was not vacuous.
    assert!(Program::<NoExt>::from_bytes(&bytes).is_ok());
}

/// A count read from the input must not be trusted as a capacity: a handful of
/// bytes claiming four billion instructions must fail, not allocate.
#[test]
fn a_huge_declared_count_fails_rather_than_allocating() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"NHVM");
    bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes()); // frame
    bytes.extend_from_slice(&0u32.to_le_bytes()); // globals
    bytes.extend_from_slice(&0u32.to_le_bytes()); // no fns
    bytes.extend_from_slice(&u32::MAX.to_le_bytes()); // ... and four billion ops

    assert_eq!(Program::<NoExt>::from_bytes(&bytes).unwrap_err(), WireError::Truncated);
}

// ---------------------------------------------------------------------------
// Extensions cross the wire too, because a language says how
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum BasicOp {
    Mid { dst: u16, src: u16, start: u16, len: u16 },
}

impl Extension for BasicOp {
    fn exec(&self, cx: &mut ExtCx<'_>) -> Result<Flow, String> {
        match self {
            BasicOp::Mid { dst, src, start, len } => {
                let s = match cx.reg(*src) {
                    Value::Str(s) => s.clone(),
                    other => return Err(format!("MID$ wants a string, got {other:?}")),
                };
                let start = cx.reg(*start).as_num()? as usize;
                let len = cx.reg(*len).as_num()? as usize;
                let out: String = s.chars().skip(start).take(len).collect();
                cx.set(*dst, Value::str(&out));
                Ok(Flow::Next)
            }
        }
    }
}

impl Wire for BasicOp {
    fn encode(&self, out: &mut Vec<u8>) {
        match self {
            BasicOp::Mid { dst, src, start, len } => {
                out.push(0);
                for v in [dst, src, start, len] {
                    nh_vm::wire::put_u16(out, *v);
                }
            }
        }
    }
    fn decode(r: &mut Reader<'_>) -> Result<Self, WireError> {
        match r.u8()? {
            0 => Ok(BasicOp::Mid {
                dst: r.u16()?,
                src: r.u16()?,
                start: r.u16()?,
                len: r.u16()?,
            }),
            tag => Err(WireError::UnknownTag { what: "BasicOp", tag }),
        }
    }
}

/// A language's own instructions cross the boundary because the language says
/// how — which is the point of `Wire` being a trait rather than something the
/// VM guesses.
#[test]
fn an_extension_crosses_the_wire_and_still_runs() {
    let p = Program::<BasicOp> {
        code: vec![
            Op::LoadK { dst: 0, value: Value::str("NAILHAMMER") },
            Op::LoadK { dst: 1, value: Value::Num(4.0) },
            Op::LoadK { dst: 2, value: Value::Num(6.0) },
            Op::Ext(BasicOp::Mid { dst: 3, src: 0, start: 1, len: 2 }),
            Op::Print { src: 3 },
            Op::Halt,
        ],
        frame: 4,
        ..Program::default()
    };

    let back = Program::<BasicOp>::from_bytes(&p.to_bytes()).expect("decodes");
    assert_eq!(format!("{:?}", p.code), format!("{:?}", back.code));

    let globals = LocalStore::new(0);
    let mut m = Machine::new(&back, &globals);
    assert!(matches!(m.resume(), Step::Done));
    assert_eq!(m.output, ["HAMMER"], "the extension ran after a round trip");
}
