//! Where mutable shared state lives — and the one knob in the design.
//!
//! # Why this is a trait
//!
//! VM-DESIGN.md §3.7 argues most configuration should be deleted rather than
//! supported, and deletes several. This is the one that survived, on the
//! merits: a globals table that is read constantly and written rarely wants
//! different machinery from one that is written hot, the right answer depends
//! on a workload the toolkit cannot see, and picking one would be a guess.
//!
//! It is a **generic parameter** on [`Machine`](crate::Machine) rather than a
//! trait object, so the default costs no virtual call and a program that never
//! touches a global never pays for the abstraction at all.
//!
//! # Per slot, never per bank
//!
//! The failure to avoid is one lock over the whole table, which serialises
//! every program against every other for the sake of one slot. Every
//! implementation here locks per slot: writing global 3 must not block reading
//! global 4.

use std::sync::RwLock;

use crate::value::Value;

pub type Slot = u32;

pub trait SharedStore: Send + Sync {
    fn load(&self, slot: Slot) -> Value;
    fn store(&self, slot: Slot, value: Value);
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// The baseline: one `RwLock` per slot.
///
/// **This is deliberately the obvious implementation rather than the good one.**
/// VM-DESIGN.md §7.4 argues a per-slot `RwLock` is a poor default — readers
/// contend with each other on the atomic counter even when nothing writes, and
/// the guard is fatter than an `f64`. That argument is a prediction, and the
/// point of having a trait is that it can be *measured* instead of believed.
/// This is the thing to measure against.
#[derive(Debug)]
pub struct RwLockStore {
    slots: Vec<RwLock<Value>>,
}

impl RwLockStore {
    pub fn new(n: usize) -> Self {
        RwLockStore {
            slots: (0..n).map(|_| RwLock::new(Value::Nil)).collect(),
        }
    }

    /// A store that does not block, so "per slot, never per bank" can be
    /// *demonstrated* rather than asserted.
    ///
    /// Holding [`read_guard`](Self::read_guard) on one slot and calling this on
    /// another must succeed; a bank-wide lock would refuse. That is a
    /// deterministic check, where the obvious alternative — two threads and a
    /// stopwatch — would either hang or prove nothing.
    pub fn try_store(&self, slot: Slot, value: Value) -> bool {
        match self.slots[slot as usize].try_write() {
            Ok(mut g) => {
                *g = value;
                true
            }
            Err(_) => false,
        }
    }

    /// Holds one slot for reading. See [`try_store`](Self::try_store).
    pub fn read_guard(&self, slot: Slot) -> std::sync::RwLockReadGuard<'_, Value> {
        self.slots[slot as usize].read().expect("poisoned")
    }
}

impl SharedStore for RwLockStore {
    fn load(&self, slot: Slot) -> Value {
        self.slots[slot as usize]
            .read()
            .expect("a poisoned global is a bug, not a runtime condition")
            .clone()
    }

    fn store(&self, slot: Slot, value: Value) {
        *self.slots[slot as usize]
            .write()
            .expect("a poisoned global is a bug, not a runtime condition") = value;
    }

    fn len(&self) -> usize {
        self.slots.len()
    }
}

/// For programs that share nothing.
///
/// Not a niche case: a standalone language, or any program whose globals never
/// leave one thread, should pay nothing for machinery it does not use. Reading
/// an unset slot gives `Nil`, exactly as the shared stores do.
#[derive(Debug, Default)]
pub struct LocalStore {
    slots: RwLock<Vec<Value>>,
}

impl LocalStore {
    pub fn new(n: usize) -> Self {
        LocalStore {
            slots: RwLock::new(vec![Value::Nil; n]),
        }
    }
}

impl SharedStore for LocalStore {
    fn load(&self, slot: Slot) -> Value {
        self.slots.read().expect("poisoned").get(slot as usize).cloned().unwrap_or_default()
    }

    fn store(&self, slot: Slot, value: Value) {
        let mut g = self.slots.write().expect("poisoned");
        if slot as usize >= g.len() {
            g.resize(slot as usize + 1, Value::Nil);
        }
        g[slot as usize] = value;
    }

    fn len(&self) -> usize {
        self.slots.read().expect("poisoned").len()
    }
}

/// One lock over the whole table — **the thing this design exists to avoid.**
///
/// Present so the cost of getting it wrong can be measured rather than
/// asserted. VM-DESIGN.md §7.4 claims a bank-wide lock "serialises every
/// program against every other for the sake of one slot"; `examples/bench_store`
/// puts a number on that claim.
#[derive(Debug)]
pub struct BankLockStore {
    slots: RwLock<Vec<Value>>,
}

impl BankLockStore {
    pub fn new(n: usize) -> Self {
        BankLockStore {
            slots: RwLock::new(vec![Value::Nil; n]),
        }
    }
}

impl SharedStore for BankLockStore {
    fn load(&self, slot: Slot) -> Value {
        self.slots.read().expect("poisoned")[slot as usize].clone()
    }

    fn store(&self, slot: Slot, value: Value) {
        self.slots.write().expect("poisoned")[slot as usize] = value;
    }

    fn len(&self) -> usize {
        self.slots.read().expect("poisoned").len()
    }
}

/// Per-slot `Mutex` rather than `RwLock`.
///
/// The interesting comparison against [`RwLockStore`]: an `RwLock` read still
/// writes — it increments a reader counter — so two readers of the *same* slot
/// contend on a cache line even though neither mutates the value. If that cost
/// is real, this should not be much worse on read-heavy work despite excluding
/// concurrent readers outright.
#[derive(Debug)]
pub struct MutexStore {
    slots: Vec<std::sync::Mutex<Value>>,
}

impl MutexStore {
    pub fn new(n: usize) -> Self {
        MutexStore {
            slots: (0..n).map(|_| std::sync::Mutex::new(Value::Nil)).collect(),
        }
    }
}

impl SharedStore for MutexStore {
    fn load(&self, slot: Slot) -> Value {
        self.slots[slot as usize].lock().expect("poisoned").clone()
    }

    fn store(&self, slot: Slot, value: Value) {
        *self.slots[slot as usize].lock().expect("poisoned") = value;
    }

    fn len(&self) -> usize {
        self.slots.len()
    }
}

/// Numbers only, one `AtomicU64` per slot. **Lock-free in both directions.**
///
/// This is the ceiling, and it exists to answer the representation question
/// VM-DESIGN.md §7.4 left open: *whether small values live inline in an atomic
/// word or behind a pointer like everything else.* An `f64` fits in a `u64`, so
/// for numeric globals there need be no lock, no guard and no allocation at all.
///
/// It cannot hold a string, which is exactly what makes it a measurement rather
/// than a proposal: it says what the inline representation would be worth, and
/// a real store would need a hybrid to capture it.
#[derive(Debug)]
pub struct AtomicNumStore {
    slots: Vec<std::sync::atomic::AtomicU64>,
}

impl AtomicNumStore {
    pub fn new(n: usize) -> Self {
        AtomicNumStore {
            slots: (0..n).map(|_| std::sync::atomic::AtomicU64::new(f64::to_bits(0.0))).collect(),
        }
    }
}

impl SharedStore for AtomicNumStore {
    fn load(&self, slot: Slot) -> Value {
        use std::sync::atomic::Ordering;
        Value::Num(f64::from_bits(self.slots[slot as usize].load(Ordering::Acquire)))
    }

    /// A non-number is dropped rather than stored. Acceptable only because this
    /// type exists to be benchmarked, never to be a default.
    fn store(&self, slot: Slot, value: Value) {
        use std::sync::atomic::Ordering;
        if let Value::Num(n) = value {
            self.slots[slot as usize].store(f64::to_bits(n), Ordering::Release);
        }
    }

    fn len(&self) -> usize {
        self.slots.len()
    }
}

/// The default, chosen by measurement rather than intuition.
///
/// `examples/bench_store` says a per-slot `Mutex` matches or beats a per-slot
/// `RwLock` **even on read-heavy work** — 136 vs 109 M ops/s at four threads,
/// 181 vs 145 single-threaded on an M4. An `RwLock` read is a write: it bumps a
/// reader counter, so two readers of one slot contend on a cache line, and the
/// guard is fatter than the `f64` it protects. Reaching for `RwLock` because a
/// workload reads a lot is exactly wrong here.
///
/// This is the best *safe, general* store in this module: it holds any `Value`,
/// needs no unsafe, and is per slot. [`AtomicNumStore`] is 4–40× faster and
/// cannot hold a string, which is why the next step is a hybrid rather than a
/// swap. See VM-DESIGN.md §7.4.
pub type DefaultStore = MutexStore;
