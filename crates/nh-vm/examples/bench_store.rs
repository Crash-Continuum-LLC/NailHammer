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

use nh_vm::{
    AtomicNumStore, BankLockStore, HybridStore, MutexStore, RwLockStore, SharedStore, Slot, Value,
};

const SLOTS: usize = 64;

/// Big enough that a run takes tens of milliseconds.
///
/// The first version of this used 200k, which finished in 0.3–1.4 ms — and at
/// that scale thread spawn, cache warmup and CPU frequency scaling dominated
/// the measurement. Two runs disagreed about which of `RwLock` and `Mutex` was
/// faster, by a factor of two, in opposite directions. A decision was made on
/// that data and had to be withdrawn.
const OPS_PER_THREAD: usize = 5_000_000;

/// Each configuration runs this many times and the **median** is reported.
///
/// Median rather than mean: one descheduled run should not move the number, and
/// one lucky run should not either.
const REPEATS: usize = 5;

/// A sharded concurrent map, implementing [`SharedStore`] **from outside the
/// crate**.
///
/// Two things are being demonstrated at once. The obvious one is whether
/// sharded hashing beats a per-slot lock. The less obvious one matters more:
/// this lives in an example, backed by a dev-dependency, so `nh-vm` itself
/// still depends on nothing. A host that wants DashMap brings it — the trait is
/// the seam, and it works from the outside.
///
/// It also corrects a claim in `op.rs`. That comment rejected name-keyed
/// globals because "a map lookup under a lock held across the hash" would be
/// bank-wide contention. That conflates *a map* with *one lock over a map*: a
/// sharded map holds no such lock. The real argument for slots is that an index
/// beats a hash, which is a narrower claim — and one this measures.
struct DashStore {
    map: dashmap::DashMap<Slot, Value>,
}

impl DashStore {
    fn new(n: usize) -> Self {
        let map = dashmap::DashMap::new();
        for i in 0..n {
            map.insert(i as Slot, Value::Nil);
        }
        DashStore { map }
    }
}

impl SharedStore for DashStore {
    fn load(&self, slot: Slot) -> Value {
        self.map.get(&slot).map(|v| v.clone()).unwrap_or(Value::Nil)
    }

    fn store(&self, slot: Slot, value: Value) {
        self.map.insert(slot, value);
    }

    fn len(&self) -> usize {
        self.map.len()
    }
}

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
            run::<DashStore>("DashMap (sharded)         ", threads, read_pct, Spread::Wide);
            run::<HybridStore>("hybrid (num lock-free)    ", threads, read_pct, Spread::Wide);
            println!();
        }
        if threads > 1 {
            println!("  contended   (every thread on ONE slot, 95% read)");
            run::<BankLockStore>("bank RwLock (anti-pattern)", threads, 95, Spread::OneSlot);
            run::<RwLockStore>("per-slot RwLock (default) ", threads, 95, Spread::OneSlot);
            run::<MutexStore>("per-slot Mutex            ", threads, 95, Spread::OneSlot);
            run::<AtomicNumStore>("per-slot AtomicU64        ", threads, 95, Spread::OneSlot);
            run::<DashStore>("DashMap (sharded)         ", threads, 95, Spread::OneSlot);
            run::<HybridStore>("hybrid (num lock-free)    ", threads, 95, Spread::OneSlot);
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
impl Build for DashStore {
    fn build(n: usize) -> Self {
        DashStore::new(n)
    }
}
impl Build for HybridStore {
    fn build(n: usize) -> Self {
        HybridStore::new(n)
    }
}

fn run<S: Build>(label: &str, threads: usize, read_pct: usize, spread: Spread) {
    // One warmup pass, discarded: it pays for page faults, first-touch on the
    // slot array, and getting the cores off their idle clock.
    let _ = once::<S>(threads, read_pct, spread, OPS_PER_THREAD / 10);

    let mut rates: Vec<f64> = (0..REPEATS)
        .map(|_| once::<S>(threads, read_pct, spread, OPS_PER_THREAD))
        .collect();
    rates.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let median = rates[REPEATS / 2];
    let spread_pct = (rates[REPEATS - 1] - rates[0]) / median * 100.0;

    // The spread is printed rather than hidden. A row with ±40% variance has
    // not measured anything, and the reader should be able to see that rather
    // than trusting a tidy median.
    println!("    {label}  {median:>8.1} M ops/s   (±{spread_pct:>4.0}%)");
}

fn once<S: Build>(threads: usize, read_pct: usize, spread: Spread, ops: usize) -> f64 {
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
            for i in 0..ops {
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

    // `sink` is consumed so the reads cannot be optimised away.
    if sink == f64::INFINITY {
        println!("unreachable");
    }
    (threads * ops) as f64 / elapsed.as_secs_f64() / 1_000_000.0
}
