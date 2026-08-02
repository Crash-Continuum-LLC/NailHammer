//! Runs a bytecode file, having never seen the language it came from.
//!
//! The host half. It knows `nh-vm` and nothing about `vm-c`'s grammar, parser,
//! or handlers — the boundary is the bytes on disk.
use nh_vm::{DefaultStore, Machine, NoExt, Program, Step};

fn main() {
    let path = std::env::args().nth(1).expect("usage: execute <file.nhb>");
    let bytes = std::fs::read(&path).expect("reading bytecode");

    let program = Program::<NoExt>::from_bytes(&bytes).unwrap_or_else(|e| {
        eprintln!("cannot load `{path}`: {e}");
        std::process::exit(1);
    });

    let globals = DefaultStore::new(program.globals);
    let mut m = Machine::new(&program, &globals);
    match m.resume() {
        Step::Done => {
            for line in &m.output {
                println!("{line}");
            }
        }
        Step::Failed(e) => {
            eprintln!("runtime error: {e}");
            std::process::exit(1);
        }
        Step::Awaiting(_) => eprintln!("this program suspended; a driver would resume it"),
    }
}
