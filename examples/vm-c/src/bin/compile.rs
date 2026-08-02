//! Compiles source to a bytecode file and stops.
//!
//! This is the plugin half: it never constructs a `Machine`, never links an
//! execution engine, and hands off bytes (VM-DESIGN.md §8.1).
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [src, out] = args.as_slice() else {
        eprintln!("usage: compile <source.c> <out.nhb>");
        std::process::exit(2);
    };
    let text = std::fs::read_to_string(src).expect("reading source");
    let program = vm_c::compile(&text).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });
    std::fs::write(out, program.to_bytes()).expect("writing bytecode");
    eprintln!("ok: {} instructions", program.code.len());
}
