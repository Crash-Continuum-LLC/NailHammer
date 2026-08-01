//! What the prototype is supposed to prove.
//!
//! These are not coverage tests. Each one corresponds to a claim in
//! `VM-DESIGN.md` that would sink the design if it turned out to be false, and
//! the point of writing the crate at all was to find out.

use std::sync::Arc;
use std::thread;

use nh_vm::{
    Cmp, ExtCx, Extension, Flow, LocalStore, Machine, NoExt, Op, Program, RwLockStore, SharedStore,
    Step, Value,
};

/// Most tests here care about instructions, not about the shape a program is
/// delivered in.
fn program<X>(code: Vec<Op<X>>, frame: usize) -> Program<X> {
    Program { code, frame, ..Program::default() }
}

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
    let prog = program(code, 3);
    let mut m = Machine::new(&prog, &globals);

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
    let prog = program(code, 4);
    let mut m = Machine::new(&prog, &globals);

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
                let prog = program(code, 3);
    let mut m = Machine::new(&prog, &*globals);
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
    let prog = program(code, 4);
    let mut m = Machine::new(&prog, &globals);

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
    let prog = program(code, 2);
    let mut m = Machine::new(&prog, &globals);

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
    let prog = program(code, 4);
    let mut m = Machine::new(&prog, &globals);

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
    let prog = program(code, 3);
    let mut m = Machine::new(&prog, &globals);

    match m.resume() {
        Step::Failed(e) => assert!(e.contains("division by zero"), "{e}"),
        other => panic!("expected a failure, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// HybridStore — a lock-free read path is where correctness bugs hide
// ---------------------------------------------------------------------------

/// Values must survive a round trip through both paths, and switching a slot
/// between them must not leave the other path's stale value visible.
#[test]
fn the_hybrid_store_round_trips_both_paths() {
    let s = nh_vm::HybridStore::new(2);

    s.store(0, Value::Num(1.5));
    assert_eq!(s.load(0), Value::Num(1.5), "fast path");

    s.store(0, Value::str("now a string"));
    assert_eq!(s.load(0), Value::str("now a string"), "switched to the slow path");

    s.store(0, Value::Num(2.5));
    assert_eq!(s.load(0), Value::Num(2.5), "and back — the string must not linger");

    s.store(1, Value::Bool(true));
    assert_eq!(s.load(1), Value::Bool(true));
    s.store(1, Value::Nil);
    assert_eq!(s.load(1), Value::Nil);
}

/// A value whose bits collide with the fast path's sentinel round-trips
/// **exactly** — same payload, not merely "still a NaN".
///
/// This is the assertion worth making, and it is stronger than the obvious one.
/// A first version checked only `is_nan()`, which passed whether or not the
/// implementation rewrote the caller's value — so it proved nothing about the
/// thing it was named after. Every NaN-boxing scheme canonicalises this case;
/// this one must not, because `heavy` is authoritative and the collision costs
/// only a trip down the slow path.
#[test]
fn a_value_colliding_with_the_sentinel_is_returned_unchanged() {
    let s = nh_vm::HybridStore::new(1);
    let sentinel = f64::from_bits(0xFFF8_0000_DEAD_0001);
    assert!(sentinel.is_nan(), "the sentinel is a NaN payload");

    s.store(0, Value::Num(sentinel));

    match s.load(0) {
        Value::Num(n) => assert_eq!(
            n.to_bits(),
            sentinel.to_bits(),
            "the caller's NaN payload was rewritten"
        ),
        other => panic!("a stored number came back as {other:?}"),
    }
}

#[test]
fn an_ordinary_nan_round_trips_bit_for_bit() {
    let s = nh_vm::HybridStore::new(1);
    s.store(0, Value::Num(f64::NAN));
    match s.load(0) {
        Value::Num(n) => assert_eq!(n.to_bits(), f64::NAN.to_bits()),
        other => panic!("{other:?}"),
    }
}

/// Concurrent readers on the lock-free path, while a writer flips the slot
/// between representations. Every read must yield *some* value that was stored
/// — never a torn one, and never a panic.
#[test]
fn the_hybrid_store_is_safe_under_concurrent_flips() {
    let s = Arc::new(nh_vm::HybridStore::new(1));
    s.store(0, Value::Num(0.0));

    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));

    let writer = {
        let s = Arc::clone(&s);
        let stop = Arc::clone(&stop);
        thread::spawn(move || {
            for i in 0..20_000 {
                if i % 2 == 0 {
                    s.store(0, Value::Num(i as f64));
                } else {
                    s.store(0, Value::str("flip"));
                }
            }
            stop.store(true, std::sync::atomic::Ordering::Relaxed);
        })
    };

    let mut readers = Vec::new();
    for _ in 0..3 {
        let s = Arc::clone(&s);
        let stop = Arc::clone(&stop);
        readers.push(thread::spawn(move || {
            let mut seen_num = false;
            let mut seen_str = false;
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                match s.load(0) {
                    Value::Num(_) => seen_num = true,
                    Value::Str(t) => {
                        assert_eq!(&*t, "flip", "an invented string appeared");
                        seen_str = true;
                    }
                    other => panic!("a value nobody stored: {other:?}"),
                }
            }
            (seen_num, seen_str)
        }));
    }

    writer.join().unwrap();
    for r in readers {
        r.join().expect("a reader panicked — the fast path is not safe");
    }
}

// ---------------------------------------------------------------------------
// Calls — the gap that made the VM less capable than the scaffold it replaces
// ---------------------------------------------------------------------------

use std::collections::HashMap;
use nh_vm::FnDef;

/// `fn double(n) = n + n`, called twice.
///
/// The calling convention is a copy with no names in it: arguments sit in
/// `base .. base + argc` and become the callee's slots `0..argc`.
#[test]
fn a_function_is_called_and_returns() {
    // 0: LoadK r0, 21      <- argument
    // 1: Call r0 <- double(base=0, argc=1)
    // 2: Print r0
    // 3: Halt
    // 4: double:  Add r1 <- r0 + r0
    // 5:          Return r1
    let code: Vec<Op<NoExt>> = vec![
        Op::LoadK { dst: 0, value: Value::Num(21.0) },
        Op::Call { dst: 0, base: 0, argc: 1, key: "double".into(), shown: "double".into() },
        Op::Print { src: 0 },
        Op::Halt,
        Op::Add { dst: 1, a: 0, b: 0 },
        Op::Return { src: 1 },
    ];
    let mut fns = HashMap::new();
    fns.insert("double".to_string(), FnDef { addr: 4, arity: 1, frame: 2 });

    let prog = Program { code, fns, frame: 2, globals: 0 };
    let globals = LocalStore::new(0);
    let mut m = Machine::new(&prog, &globals);

    assert!(matches!(m.resume(), Step::Done));
    assert_eq!(m.output, ["42"]);
}

/// Recursion, which is the reason functions are looked up **by name at run
/// time** rather than patched at compile time: a body can call itself before
/// the compiler has finished emitting it.
#[test]
fn a_function_can_call_itself() {
    // fact(n) = if n < 2 { 1 } else { n * fact(n - 1) }
    let code: Vec<Op<NoExt>> = vec![
        Op::LoadK { dst: 0, value: Value::Num(5.0) },
        Op::Call { dst: 0, base: 0, argc: 1, key: "fact".into(), shown: "fact".into() },
        Op::Print { src: 0 },
        Op::Halt,
        // fact: r0 = n
        Op::LoadK { dst: 1, value: Value::Num(2.0) },          // 4
        Op::Compare { dst: 1, cmp: Cmp::Lt, a: 0, b: 1 },      // 5
        Op::JumpIfFalse { src: 1, target: 9 },                 // 6
        Op::LoadK { dst: 1, value: Value::Num(1.0) },          // 7
        Op::Return { src: 1 },                                 // 8
        Op::LoadK { dst: 1, value: Value::Num(1.0) },          // 9
        Op::Sub { dst: 1, a: 0, b: 1 },                        // 10  n - 1
        Op::Call { dst: 1, base: 1, argc: 1, key: "fact".into(), shown: "fact".into() }, // 11
        Op::Mul { dst: 1, a: 0, b: 1 },                        // 12  n * fact(n-1)
        Op::Return { src: 1 },                                 // 13
    ];
    let mut fns = HashMap::new();
    fns.insert("fact".to_string(), FnDef { addr: 4, arity: 1, frame: 3 });

    let prog = Program { code, fns, frame: 2, globals: 0 };
    let globals = LocalStore::new(0);
    let mut m = Machine::new(&prog, &globals);

    assert!(matches!(m.resume(), Step::Done));
    assert_eq!(m.output, ["120"], "5! = 120");
}

/// Runaway recursion must fail the *program*, not the process. A host running
/// several languages cannot have one of them take the native stack down.
#[test]
fn infinite_recursion_fails_the_program_not_the_host() {
    let code: Vec<Op<NoExt>> = vec![
        Op::Call { dst: 0, base: 0, argc: 0, key: "loop".into(), shown: "loop".into() },
        Op::Halt,
        Op::Call { dst: 0, base: 0, argc: 0, key: "loop".into(), shown: "loop".into() },
        Op::Return { src: 0 },
    ];
    let mut fns = HashMap::new();
    fns.insert("loop".to_string(), FnDef { addr: 2, arity: 0, frame: 1 });

    let prog = Program { code, fns, frame: 1, globals: 0 };
    let globals = LocalStore::new(0);
    let mut m = Machine::new(&prog, &globals);

    match m.resume() {
        Step::Failed(e) => assert!(e.contains("call stack exceeded"), "{e}"),
        other => panic!("expected a failure, got {other:?}"),
    }
}

#[test]
fn calling_something_undefined_names_it() {
    let code: Vec<Op<NoExt>> = vec![
        Op::Call { dst: 0, base: 0, argc: 0, key: "nope".into(), shown: "NOPE".into() },
        Op::Halt,
    ];
    let prog = Program { code, frame: 1, ..Program::default() };
    let globals = LocalStore::new(0);
    let mut m = Machine::new(&prog, &globals);

    match m.resume() {
        // `shown` rather than `key`: a case-folding language stores `nope` and
        // the user wrote `NOPE`, and the message is for the user.
        Step::Failed(e) => assert!(e.contains("`NOPE`"), "{e}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_wrong_number_of_arguments_is_reported() {
    let code: Vec<Op<NoExt>> = vec![
        Op::Call { dst: 0, base: 0, argc: 3, key: "f".into(), shown: "f".into() },
        Op::Halt,
        Op::ReturnUnit,
    ];
    let mut fns = HashMap::new();
    fns.insert("f".to_string(), FnDef { addr: 2, arity: 1, frame: 1 });

    let prog = Program { code, fns, frame: 4, globals: 0 };
    let globals = LocalStore::new(0);
    let mut m = Machine::new(&prog, &globals);

    match m.resume() {
        Step::Failed(e) => assert!(e.contains("takes 1 argument(s), got 3"), "{e}"),
        other => panic!("{other:?}"),
    }
}

/// `+` concatenates when either side is a string — a VM decision, so every
/// language on it gets the same answer.
#[test]
fn add_concatenates_strings() {
    let code: Vec<Op<NoExt>> = vec![
        Op::LoadK { dst: 0, value: Value::str("nail") },
        Op::LoadK { dst: 1, value: Value::str("hammer") },
        Op::Add { dst: 2, a: 0, b: 1 },
        Op::Print { src: 2 },
        Op::LoadK { dst: 0, value: Value::str("v") },
        Op::LoadK { dst: 1, value: Value::Num(2.0) },
        Op::Add { dst: 2, a: 0, b: 1 },
        Op::Print { src: 2 },
        Op::Halt,
    ];
    let prog = program(code, 3);
    let globals = LocalStore::new(0);
    let mut m = Machine::new(&prog, &globals);

    assert!(matches!(m.resume(), Step::Done));
    assert_eq!(m.output, ["nailhammer", "v2"]);
}
