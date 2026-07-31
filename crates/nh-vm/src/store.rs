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

/// Numbers lock-free, everything else behind a per-slot lock.
///
/// # Why
///
/// [`AtomicNumStore`] is 3–28× faster than any lock here and cannot hold a
/// string, so it measures a ceiling rather than offering a design. This reaches
/// for that ceiling in the case that matters — a numeric global, read often —
/// while still holding any [`Value`].
///
/// # How it stays correct
///
/// Each slot is an `AtomicU64` beside an `RwLock<Value>`. The atomic holds
/// either the bits of an `f64` or a sentinel meaning *look in the lock*.
///
/// * **Writes always take the lock**, set the value, and store the tag **last**,
///   with `Release`.
/// * **Reads load the tag first**, with `Acquire`. A number is returned there
///   and then — no lock, no guard, no contention with other readers. Anything
///   else falls through to the lock.
///
/// Taking the tag store as the linearisation point of a write makes this
/// linearisable: a reader that loads the tag before that store observes the
/// previous value and orders before the write; one that loads it after either
/// reads the new number directly or takes the lock, which the writer has
/// released. There is no window in which a torn or invented value is visible.
///
/// # The sentinel collision is benign, and that is not an accident
///
/// An `f64` uses all 64 bits, so the sentinel has to be a NaN payload — and a
/// program can store exactly that NaN. Every NaN-boxing implementation handles
/// this by canonicalising such a value to the standard quiet NaN.
///
/// **This one does not, because it does not have to.** `heavy` is written on
/// *every* store, including numeric ones, so the slow path always holds the
/// truth. A slot whose bits collide with the sentinel simply takes the slow
/// path: correct, and slower by one lock for one unlucky value.
///
/// Canonicalising would be worse than useless here. It buys nothing —
/// correctness already holds — and it costs the caller their NaN payload,
/// silently rewriting a value they stored. Keeping `heavy` authoritative is
/// what makes the fast path a pure optimisation rather than a second source of
/// truth, and a second source of truth is where this design would have gone
/// wrong.
#[derive(Debug)]
pub struct HybridStore {
    slots: Vec<HybridSlot>,
}

#[derive(Debug)]
struct HybridSlot {
    /// `f64` bits, or [`NOT_A_NUMBER_SLOT`].
    tag: std::sync::atomic::AtomicU64,
    heavy: RwLock<Value>,
}

/// The sentinel: a quiet NaN with a payload nothing produces by accident.
const NOT_A_NUMBER_SLOT: u64 = 0xFFF8_0000_DEAD_0001;

impl HybridStore {
    pub fn new(n: usize) -> Self {
        HybridStore {
            slots: (0..n)
                .map(|_| HybridSlot {
                    tag: std::sync::atomic::AtomicU64::new(NOT_A_NUMBER_SLOT),
                    heavy: RwLock::new(Value::Nil),
                })
                .collect(),
        }
    }
}

impl SharedStore for HybridStore {
    fn load(&self, slot: Slot) -> Value {
        use std::sync::atomic::Ordering;
        let s = &self.slots[slot as usize];
        let bits = s.tag.load(Ordering::Acquire);
        if bits != NOT_A_NUMBER_SLOT {
            // The fast path, and the whole point: no lock is taken at all.
            return Value::Num(f64::from_bits(bits));
        }
        s.heavy.read().expect("poisoned").clone()
    }

    fn store(&self, slot: Slot, value: Value) {
        use std::sync::atomic::Ordering;
        let s = &self.slots[slot as usize];
        let mut g = s.heavy.write().expect("poisoned");

        // The tag is stored last, under the lock, so it is the point at which
        // the write becomes visible.
        match value {
            Value::Num(n) => {
                *g = Value::Num(n);
                s.tag.store(n.to_bits(), Ordering::Release);
            }
            other => {
                *g = other;
                s.tag.store(NOT_A_NUMBER_SLOT, Ordering::Release);
            }
        }
    }

    fn len(&self) -> usize {
        self.slots.len()
    }
}

/// The default: [`HybridStore`], which wins everywhere measured.
///
/// `examples/bench_store` (M4, median of 5 × 5M ops/thread, read-heavy):
///
/// | threads | RwLock | Mutex | DashMap | **hybrid** | *AtomicU64 ceiling* |
/// |---|---|---|---|---|---|
/// | 1 | 394 | 201 | 143 | **538** | *612* |
/// | 2 | 95 | 114 | 115 | **580** | *869* |
/// | 4 | 89 | 158 | 100 | **949** | *1013* |
/// | 8 | 61 | 74 | 88 | **485** | *509* |
///
/// The lock-free read path holds **88–95% of the numbers-only ceiling** while
/// still storing any [`Value`], and beats the best lock by 8× at eight threads.
/// The tag check does not eat the advantage, which was the open question.
///
/// Its gain on **write-heavy** work is much smaller — 1.2–1.7× — because writes
/// still take the lock. That is the honest limit of this design and the next
/// thing worth attacking.
///
/// [`SharedStore`] remains a trait, but for a better reason than "no default is
/// right". Globals that are dynamic, sparse, or shared *by name* between
/// independently loaded languages want a map, and [`DashMap`-backed stores are
/// what that looks like](https://docs.rs/dashmap) — a case the hybrid does not
/// serve at all, rather than serves worse.
pub type DefaultStore = HybridStore;
