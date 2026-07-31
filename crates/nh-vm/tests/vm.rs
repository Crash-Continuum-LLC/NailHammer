//! What the prototype is supposed to prove.
//!
//! These are not coverage tests. Each one corresponds to a claim in
//! `VM-DESIGN.md` that would sink the design if it turned out to be false, and
//! the point of writing the crate at all was to find out.

use std::sync::Arc;
use std::thread;

use nh_vm::{Cmp, ExtCx, Extension, Flow, LocalStore, Machine, NoExt, Op, RwLockStore, SharedStore, Step, Value};

// ---------------------------------------------------------------------------
// §7.3 — a language with no extensions pays nothing
// ---------------------------------------------------------------------------

/// `NoExt` is uninhabited, so `Op::Ext` cannot be constructed for a language
/// that has no commands of its own. If this needed a match arm at the call
/// site, "extending costs nothing to those who do not" would be false.
#[test]
fn a_language_without_extensions_runs() {
    let code: Vec<Op<NoExt>> = vec![
        Op::LoadK { dst: 0, value: Value::Num(6.0) },
        Op::LoadK { dst: 1, value: Value::Num(7.0) },
        Op::Mul { dst: 2, a: 0, b: 1 },
        Op::Print { src: 2 },
        Op::Halt,
    ];
    let globals = LocalStore::new(0);
    let mut m = Machine::new(&code, &globals, 3);

    assert!(matches!(m.resume(), Step::Done));
    assert_eq!(m.output, ["42"]);
}

// ---------------------------------------------------------------------------
// §7.3 — a language adds commands without forking the machine
// ---------------------------------------------------------------------------

/// A BASIC-ish `MID$`, as a language's own instruction. The point is that
/// nothing in `nh-vm` knows this exists, and nothing had to change to allow it.
#[derive(Clone, Debug)]
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
                // 0-based, because indexing is mandated 0-based (§3.7).
                let out: String = s.chars().skip(start).take(len).collect();
                cx.set(*dst, Value::str(&out));
                Ok(Flow::Next)
            }
        }
    }
}

#[test]
fn a_language_can_add_its_own_commands() {
    let code: Vec<Op<BasicOp>> = vec![
        Op::LoadK { dst: 0, value: Value::str("NAILHAMMER") },
        Op::LoadK { dst: 1, value: Value::Num(4.0) },
        Op::LoadK { dst: 2, value: Value::Num(6.0) },
        Op::Ext(BasicOp::Mid { dst: 3, src: 0, start: 1, len: 2 }),
        Op::Print { src: 3 },
        Op::Halt,
    ];
    let globals = LocalStore::new(0);
    let mut m = Machine::new(&code, &globals, 4);

    assert!(matches!(m.resume(), Step::Done));
    assert_eq!(m.output, ["HAMMER"], "0-based, so index 4 is the fifth character");
}

// ---------------------------------------------------------------------------
// §7.4 — the claim the whole design rests on
// ---------------------------------------------------------------------------

/// Two machines, running on two threads, over one set of globals.
///
/// This is the test that had to pass for any of the rest to matter. If a VM
/// value could not cross a thread, "programs will have shared data across
/// threads" would be a wish rather than a design.
#[test]
fn two_programs_share_globals_across_threads() {
    let globals = Arc::new(RwLockStore::new(4));

    // Slot 0 starts at zero; each thread adds its own number to it a few times.
    globals.store(0, Value::Num(0.0));

    let mut handles = Vec::new();
    for n in [1.0f64, 10.0] {
        let globals = Arc::clone(&globals);
        handles.push(thread::spawn(move || {
            for _ in 0..100 {
                // read-modify-write through the shared store
                let cur = globals.load(0).as_num().unwrap();
                let code: Vec<Op<NoExt>> = vec![
                    Op::LoadK { dst: 0, value: Value::Num(cur) },
                    Op::LoadK { dst: 1, value: Value::Num(n) },
                    Op::Add { dst: 2, a: 0, b: 1 },
                    Op::StoreGlobal { slot: 0, src: 2 },
                    Op::Halt,
                ];
                let mut m = Machine::new(&code, &*globals, 3);
                assert!(matches!(m.resume(), Step::Done));
            }
        }));
    }
    for h in handles {
        h.join().expect("a machine panicked on a worker thread");
    }

    // Not asserting an exact total: this is a deliberate read-modify-write
    // race, and the claim under test is that two machines can share state at
    // all, not that unguarded increments are atomic. What must hold is that
    // the value moved and stayed a number.
    let end = globals.load(0).as_num().unwrap();
    assert!(end > 0.0, "both machines wrote through the shared store: {end}");
}

/// Writing one global must not block reading another.
///
/// This is the "per slot, never per bank" requirement, and it is the difference
/// between a concurrent VM and a serialised one. Checked without threads: hold
/// slot 0 for reading, then prove a *non-blocking* write to slot 1 succeeds and
/// one to slot 0 does not. A bank-wide lock fails the first assertion, and the
/// second confirms the guard being held is real rather than the test proving
/// nothing.
#[test]
fn one_slot_does_not_block_another() {
    let globals = RwLockStore::new(2);
    globals.store(0, Value::Num(1.0));

    let held = globals.read_guard(0);

    assert!(
        globals.try_store(1, Value::Num(2.0)),
        "writing slot 1 must not block on a reader of slot 0"
    );
    assert!(
        !globals.try_store(0, Value::Num(9.0)),
        "and the read guard on slot 0 is genuinely held"
    );

    drop(held);
    assert_eq!(globals.load(1), Value::Num(2.0));
    assert!(globals.try_store(0, Value::Num(9.0)), "released");
}

// ---------------------------------------------------------------------------
// Suspension — carried over from the scaffolded machine, must still work
// ---------------------------------------------------------------------------

/// `Await` stops the machine and hands out a value; the driver resumes it. No
/// runtime is mentioned anywhere, which is what lets a host schedule several
/// suspended programs without the VM knowing a scheduler exists.
#[test]
fn a_program_suspends_and_resumes() {
    let code: Vec<Op<NoExt>> = vec![
        Op::LoadK { dst: 0, value: Value::Num(7.0) },
        Op::Await { dst: 1, src: 0 },
        Op::LoadK { dst: 2, value: Value::Num(1.0) },
        Op::Add { dst: 3, a: 1, b: 2 },
        Op::Print { src: 3 },
        Op::Halt,
    ];
    let globals = LocalStore::new(0);
    let mut m = Machine::new(&code, &globals, 4);

    match m.resume() {
        Step::Awaiting(v) => assert_eq!(v, Value::Num(7.0), "handed out what it was waiting on"),
        other => panic!("expected a suspension, got {other:?}"),
    }

    // The driver resolves it however it likes — here, doubling it.
    m.resume_with(Value::Num(14.0));

    assert!(matches!(m.resume(), Step::Done));
    assert_eq!(m.output, ["15"], "resumed with 14, added 1");
}

// ---------------------------------------------------------------------------
// §3.5 — truthiness is the VM's, and there is exactly one of it
// ---------------------------------------------------------------------------

#[test]
fn the_vm_owns_truthiness() {
    // Empty string is false; a non-empty one is true. A language cannot bring
    // its own rule, which is the point: `JumpIfFalse` asks `Value::truthy` and
    // nothing else.
    let code: Vec<Op<NoExt>> = vec![
        Op::LoadK { dst: 0, value: Value::str("") },
        Op::JumpIfFalse { src: 0, target: 3 },
        Op::Halt,
        Op::LoadK { dst: 1, value: Value::str("empty is false") },
        Op::Print { src: 1 },
        Op::Halt,
    ];
    let globals = LocalStore::new(0);
    let mut m = Machine::new(&code, &globals, 2);

    assert!(matches!(m.resume(), Step::Done));
    assert_eq!(m.output, ["empty is false"]);
}

#[test]
fn comparison_and_control_flow() {
    // if 3 < 5 { print "yes" }
    let code: Vec<Op<NoExt>> = vec![
        Op::LoadK { dst: 0, value: Value::Num(3.0) },
        Op::LoadK { dst: 1, value: Value::Num(5.0) },
        Op::Compare { dst: 2, cmp: Cmp::Lt, a: 0, b: 1 },
        Op::JumpIfFalse { src: 2, target: 6 },
        Op::LoadK { dst: 3, value: Value::str("yes") },
        Op::Print { src: 3 },
        Op::Halt,
    ];
    let globals = LocalStore::new(0);
    let mut m = Machine::new(&code, &globals, 4);

    assert!(matches!(m.resume(), Step::Done));
    assert_eq!(m.output, ["yes"]);
}

#[test]
fn a_failure_stops_the_machine_with_a_reason() {
    let code: Vec<Op<NoExt>> = vec![
        Op::LoadK { dst: 0, value: Value::Num(1.0) },
        Op::LoadK { dst: 1, value: Value::Num(0.0) },
        Op::Div { dst: 2, a: 0, b: 1 },
        Op::Halt,
    ];
    let globals = LocalStore::new(0);
    let mut m = Machine::new(&code, &globals, 3);

    match m.resume() {
        Step::Failed(e) => assert!(e.contains("division by zero"), "{e}"),
        other => panic!("expected a failure, got {other:?}"),
    }
}
