//! An extensible bytecode VM.
//!
//! # What this is for
//!
//! Today `nh init` writes a whole VM into each project — opcodes, machine and
//! all — so every NailHammer language invents its own bytecode and ships its
//! own interpreter. That is right for a standalone language and wrong for a
//! host that wants to load languages as plugins, because two of them produce
//! mutually unintelligible output.
//!
//! This crate is the other half: **one machine that languages extend** rather
//! than many machines that must be described to each other. See `VM-DESIGN.md`
//! at the repository root, particularly §7.
//!
//! ```
//! use nh_vm::{Machine, NoExt, Op, Step, Value, LocalStore};
//!
//! let code: Vec<Op<NoExt>> = vec![
//!     Op::LoadK { dst: 0, value: Value::Num(2.0) },
//!     Op::LoadK { dst: 1, value: Value::Num(3.0) },
//!     Op::Add { dst: 2, a: 0, b: 1 },
//!     Op::Print { src: 2 },
//!     Op::Halt,
//! ];
//!
//! let globals = LocalStore::new(0);
//! let mut m = Machine::new(&code, &globals, 3);
//! assert!(matches!(m.resume(), Step::Done));
//! assert_eq!(m.output, ["5"]);
//! ```
//!
//! # Status
//!
//! **Prototype.** It exists to find out whether the design in `VM-DESIGN.md`
//! survives contact with code, and to give the open question there — how
//! mutable shared slots should be synchronised — something to measure instead
//! of something to argue about.

pub mod machine;
pub mod op;
pub mod store;
pub mod value;

pub use machine::{Machine, Step};
pub use op::{Cmp, ExtCx, Extension, Flow, NoExt, Op, Reg};
pub use store::{
    AtomicNumStore, BankLockStore, DefaultStore, HybridStore, LocalStore, MutexStore, RwLockStore,
    SharedStore, Slot,
};
pub use value::Value;
