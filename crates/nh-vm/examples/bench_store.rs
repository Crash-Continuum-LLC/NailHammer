//! Measures the one open question in VM-DESIGN.md §7.4.
//!
//!     cargo run --release --example bench_store
//!
//! The design makes three claims about shared-slot synchronisation and calls
//! all three predictions. This puts numbers on them:
//!
//! 1. a bank-wide lock serialises every program against every other;
//! 2. a per-slot `RwLock` is a poor default, because a read still writes — it
//!    bumps a reader counter, so readers contend even with no writer present;
//! 3. small values inline in an atomic word would be materially faster than
//!    values behind a guard.
//!
//! No dependencies and no statistical machinery: each case runs a fixed number
//! of operations and reports throughput. That is enough to separate designs
//! that differ by multiples, which is the question here. It is not enough to
//! rank two that differ by a few percent, and it should not be used to.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Instant;

use nh_vm::{AtomicNumStore, BankLockStore, MutexStore, RwLockStore, SharedStore, Value};

const SLOTS: usize = 64;
const OPS_PER_THREAD: usize = 200_000;

fn main() {
    println!(
        "nh-vm shared store — {} ops/thread, {} slots\n",
        OPS_PER_THREAD, SLOTS
    );

    for &threads in &[1usize, 2, 4, 8] {
        println!("── {threads} thread(s) ─────────────────────────────────────────");
        for &(name, read_pct) in &[("read-heavy  (95% read)", 95), ("mixed       (50% read)", 50)] {
            println!("  {name}");
            run::<BankLockStore>("bank RwLock (anti-pattern)", threads, read_pct, Spread::Wide);
            run::<RwLockStore>("per-slot RwLock (default) ", threads, read_pct, Spread::Wide);
            run::<MutexStore>("per-slot Mutex            ", threads, read_pct, Spread::Wide);
            run::<AtomicNumStore>("per-slot AtomicU64        ", threads, read_pct, Spread::Wide);
            println!();
        }
        if threads > 1 {
            println!("  contended   (every thread on ONE slot, 95% read)");
            run::<BankLockStore>("bank RwLock (anti-pattern)", threads, 95, Spread::OneSlot);
            run::<RwLockStore>("per-slot RwLock (default) ", threads, 95, Spread::OneSlot);
            run::<MutexStore>("per-slot Mutex            ", threads, 95, Spread::OneSlot);
            run::<AtomicNumStore>("per-slot AtomicU64        ", threads, 95, Spread::OneSlot);
            println!();
        }
    }

    println!("Interpretation is in VM-DESIGN.md §7.4. Numbers vary by machine;");
    println!("what matters is the ratio between rows, not the absolute rate.");
}

#[derive(Clone, Copy)]
enum Spread {
    /// Threads touch different slots — the case a per-slot design is for.
    Wide,
    /// Every thread hammers slot 0 — the case where per-slot cannot help, and
    /// the honest worst case for the whole approach.
    OneSlot,
}

trait Build: SharedStore + 'static {
    fn build(n: usize) -> Self;
}
impl Build for BankLockStore {
    fn build(n: usize) -> Self {
        BankLockStore::new(n)
    }
}
impl Build for RwLockStore {
    fn build(n: usize) -> Self {
        RwLockStore::new(n)
    }
}
impl Build for MutexStore {
    fn build(n: usize) -> Self {
        MutexStore::new(n)
    }
}
impl Build for AtomicNumStore {
    fn build(n: usize) -> Self {
        AtomicNumStore::new(n)
    }
}

fn run<S: Build>(label: &str, threads: usize, read_pct: usize, spread: Spread) {
    let store = Arc::new(S::build(SLOTS));
    for i in 0..SLOTS {
        store.store(i as u32, Value::Num(i as f64));
    }

    let gate = Arc::new(Barrier::new(threads + 1));
    let stop = Arc::new(AtomicBool::new(false));
    let mut handles = Vec::new();

    for t in 0..threads {
        let store = Arc::clone(&store);
        let gate = Arc::clone(&gate);
        let stop = Arc::clone(&stop);
        handles.push(thread::spawn(move || {
            gate.wait();
            // A cheap deterministic sequence — no RNG dependency, and the same
            // sequence for every store so the comparison is like for like.
            let mut x = (t as u64).wrapping_mul(0x9E3779B9) | 1;
            let mut sink = 0.0f64;
            for i in 0..OPS_PER_THREAD {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                let slot = match spread {
                    Spread::Wide => (x as usize) % SLOTS,
                    Spread::OneSlot => 0,
                } as u32;
                if i % 100 < read_pct {
                    if let Value::Num(n) = store.load(slot) {
                        sink += n;
                    }
                } else {
                    store.store(slot, Value::Num(x as f64));
                }
            }
            stop.store(true, Ordering::Relaxed);
            sink
        }));
    }

    gate.wait();
    let t0 = Instant::now();
    let mut sink = 0.0;
    for h in handles {
        sink += h.join().expect("worker panicked");
    }
    let elapsed = t0.elapsed();

    let total = (threads * OPS_PER_THREAD) as f64;
    let m_ops = total / elapsed.as_secs_f64() / 1_000_000.0;
    // `sink` is consumed so the reads cannot be optimised away.
    println!(
        "    {label}  {m_ops:>7.2} M ops/s   ({:>6.1} ms){}",
        elapsed.as_secs_f64() * 1000.0,
        if sink == f64::INFINITY { " " } else { "" }
    );
}
