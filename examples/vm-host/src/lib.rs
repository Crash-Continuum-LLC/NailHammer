//! A host that runs several programs at once.
//!
//! # What this is, and what it is not
//!
//! This is **not** part of the VM, and that is the point. `nh-vm` mentions no
//! runtime, no future and no thread: a machine that needs something stops and
//! says so, and whoever is driving decides what to do about it
//! (VM-DESIGN.md §7.4). This is one such driver — a cooperative round-robin —
//! and a different host could use tokio, or threads, or a priority queue,
//! without the VM changing.
//!
//! # What it knows
//!
//! Bytes and a shared store. It has never seen a grammar, does not know which
//! language produced anything it runs, and could not tell two of them apart.
//! That is what makes it a *host* rather than an interpreter for one language.

use std::collections::HashMap;

use nh_vm::{DefaultStore, Machine, NoExt, Program, SharedStore, Step, Value, WireError};

/// A program the host has loaded, and what it is waiting on.
pub struct Task {
    pub name: String,
    program: Program<NoExt>,
    /// Output produced so far, across every slice it has run.
    pub output: Vec<String>,
    /// Set while the task is parked, holding whatever it asked for.
    pub waiting_on: Option<Value>,
    pub done: bool,
    pub failed: Option<String>,
    /// Where it left off — program counter *and registers*. A program that
    /// suspends mid-expression has live operands in its frame.
    resume: Option<nh_vm::Snapshot>,
}

/// What a task asked for, and what the host decided to give it.
pub type Resolver = Box<dyn Fn(&str, &Value) -> Value>;

pub struct Host {
    tasks: Vec<Task>,
    globals: DefaultStore,
    /// How a suspension is answered. The VM has no opinion; this is the whole
    /// of the host's policy about waiting.
    resolve: Resolver,
}

impl Host {
    /// `globals` is the number of shared slots every task can see.
    ///
    /// One store for all of them, so two programs written in different
    /// languages share state — which is the reason the store is a trait and
    /// synchronised per slot rather than per bank.
    pub fn new(globals: usize) -> Self {
        Host {
            tasks: Vec::new(),
            globals: DefaultStore::new(globals),
            resolve: Box::new(|_, v| v.clone()),
        }
    }

    /// Replaces the policy for answering a suspension.
    pub fn resolving(mut self, f: impl Fn(&str, &Value) -> Value + 'static) -> Self {
        self.resolve = Box::new(f);
        self
    }

    /// Loads bytecode. The only thing the host is told about a language.
    pub fn load(&mut self, name: &str, bytes: &[u8]) -> Result<(), WireError> {
        let program = Program::<NoExt>::from_bytes(bytes)?;
        self.tasks.push(Task {
            name: name.to_string(),
            program,
            output: Vec::new(),
            waiting_on: None,
            done: false,
            failed: None,
            resume: None,
        });
        Ok(())
    }

    pub fn globals(&self) -> &DefaultStore {
        &self.globals
    }

    pub fn tasks(&self) -> &[Task] {
        &self.tasks
    }

    /// Runs every task to completion, round-robin, one slice each.
    ///
    /// A slice ends when a task finishes, fails, or suspends. Returns the
    /// number of slices run, which is how a test sees *interleaving* rather
    /// than just completion: two tasks that each suspend twice take more
    /// slices than two that run straight through.
    pub fn run(&mut self) -> usize {
        let mut slices = 0;
        loop {
            let mut progressed = false;

            for i in 0..self.tasks.len() {
                if self.tasks[i].done || self.tasks[i].failed.is_some() {
                    continue;
                }
                progressed = true;
                slices += 1;
                self.slice(i);
            }

            if !progressed {
                return slices;
            }
        }
    }

    /// One slice of one task: resume it until it stops.
    ///
    /// The machine is rebuilt each slice from a [`Snapshot`](nh_vm::Snapshot)
    /// rather than held across slices, because a `Machine` borrows its program
    /// and its store — keeping one alive per task would mean self-referential
    /// structs. The snapshot carries the program counter *and the frames*,
    /// which is the part that is easy to get wrong: without the registers,
    /// `await` would work only as the first thing a program does.
    fn slice(&mut self, i: usize) {
        let task = &mut self.tasks[i];
        let mut m = Machine::new(&task.program, &self.globals);

        if let Some(s) = task.resume.take() {
            m.restore(s);
        }
        // Hand back what it asked for. What that *is* is the host's business
        // and the VM has no opinion, which is why this is a closure.
        if let Some(v) = task.waiting_on.take() {
            let answer = (self.resolve)(&task.name, &v);
            m.resume_with(answer);
        }

        match m.resume() {
            Step::Done => task.done = true,
            Step::Failed(e) => task.failed = Some(e),
            Step::Awaiting(v) => {
                task.waiting_on = Some(v);
                task.resume = Some(m.snapshot());
            }
        }
        task.output.extend(std::mem::take(&mut m.output));
    }
}

/// Everything the tasks printed, in the order the host saw it.
pub fn transcript(host: &Host) -> HashMap<String, Vec<String>> {
    host.tasks()
        .iter()
        .map(|t| (t.name.clone(), t.output.clone()))
        .collect()
}

/// Reads a global by slot, for a host that wants to see shared state.
pub fn global(host: &Host, slot: u32) -> Value {
    host.globals().load(slot)
}
