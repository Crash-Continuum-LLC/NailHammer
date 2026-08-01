//! A host running several languages at once.
//!
//! This is the thing the whole design exists to make possible, so these are
//! about the properties rather than about the API: does the host stay ignorant
//! of the languages, do suspended programs actually interleave, and do they
//! share state.

use nh_vm::Value;
use vm_host::Host;

/// The host is handed bytes and nothing else — no grammar, no parser, no idea
/// which language wrote what.
#[test]
fn one_host_runs_two_languages() {
    let c = vm_c::compile("print 2 + 3 * 4;").expect("C compiles").to_bytes();
    let basic = vm_basic::compile("PRINT 10 - 1\n").expect("BASIC compiles").to_bytes();

    let mut host = Host::new(0);
    host.load("c", &c).expect("loads");
    host.load("basic", &basic).expect("loads");
    host.run();

    let out = vm_host::transcript(&host);
    assert_eq!(out["c"], ["14"]);
    assert_eq!(out["basic"], ["9"]);
}

/// Two programs, one globals table, written in different languages.
///
/// The C program writes slot 0; the BASIC program reads it. Neither knows the
/// other exists — they agree because slots are assigned in first-seen order and
/// both name one variable.
#[test]
fn two_languages_share_one_globals_table() {
    let writer = vm_c::compile("shared = 99;").expect("C compiles").to_bytes();
    let reader = vm_basic::compile("PRINT shared\n").expect("BASIC compiles").to_bytes();

    let mut host = Host::new(4);
    host.load("writer", &writer).expect("loads");
    host.load("reader", &reader).expect("loads");
    host.run();

    assert_eq!(vm_host::global(&host, 0), Value::Num(99.0));
    assert_eq!(vm_host::transcript(&host)["reader"], ["99"]);
}

/// **Suspension actually interleaves.**
///
/// Counting slices, not output: a host that ran each program to completion in
/// turn would produce the same transcript. Three suspensions across two tasks
/// means more slices than the two a straight-through run would take, and that
/// difference is the only evidence that scheduling happened.
#[test]
fn suspended_programs_interleave_rather_than_running_in_turn() {
    // Each `await` stops the machine and hands a value to the host.
    let a = vm_c::compile("print await 1; print await 2;").expect("compiles").to_bytes();
    let b = vm_c::compile("print await 3;").expect("compiles").to_bytes();

    let mut host = Host::new(0).resolving(|_, v| match v {
        // The host decides what waiting means. Here: double it.
        Value::Num(n) => Value::Num(n * 2.0),
        other => other.clone(),
    });
    host.load("a", &a).expect("loads");
    host.load("b", &b).expect("loads");

    let slices = host.run();

    let out = vm_host::transcript(&host);
    assert_eq!(out["a"], ["2", "4"], "resumed twice, with what the host supplied");
    assert_eq!(out["b"], ["6"]);

    // `a` suspends twice and `b` once, so neither finishes in its first slice.
    assert!(slices > 2, "no interleaving happened: {slices} slices");
}

/// The registers must survive a suspension.
///
/// `1 + await 2` has `1` live in a register when the machine stops. A host that
/// restored only the program counter would resume with a garbage left operand
/// — and the sum would still *look* plausible, which is why this asserts the
/// value rather than merely that it ran.
#[test]
fn a_suspension_mid_expression_keeps_its_operands() {
    let p = vm_c::compile("print 1 + await 10;").expect("compiles").to_bytes();

    let mut host = Host::new(0).resolving(|_, v| match v {
        Value::Num(n) => Value::Num(n + 5.0),
        other => other.clone(),
    });
    host.load("t", &p).expect("loads");
    host.run();

    // 1 + (10 resolved to 15) = 16. A lost register would give 15 or nonsense.
    assert_eq!(vm_host::transcript(&host)["t"], ["16"]);
}

/// A program that fails does not take the host, or its siblings, down.
#[test]
fn one_failing_program_does_not_stop_the_others() {
    let bad = vm_c::compile("print 1 / 0;").expect("compiles").to_bytes();
    let good = vm_c::compile("print 7;").expect("compiles").to_bytes();

    let mut host = Host::new(0);
    host.load("bad", &bad).expect("loads");
    host.load("good", &good).expect("loads");
    host.run();

    let failed: Vec<&str> = host
        .tasks()
        .iter()
        .filter_map(|t| t.failed.as_deref().map(|_| t.name.as_str()))
        .collect();
    assert_eq!(failed, ["bad"]);
    assert_eq!(vm_host::transcript(&host)["good"], ["7"]);
}

/// Bytes that are not bytecode are rejected at load, not at run.
#[test]
fn a_bad_program_is_refused_when_it_is_loaded() {
    let mut host = Host::new(0);
    assert!(host.load("junk", b"definitely not bytecode").is_err());
    assert_eq!(host.tasks().len(), 0, "a refused program is not queued");
}
