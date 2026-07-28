//! Compiles a Bc program to bytecode, prints the instructions, then runs them.
//!
//! Note that this is the *same* driver an interpreter uses. `eval_source` does
//! not know or care that evaluating this host means emitting instructions —
//! that is entirely in `type Out = ()` and the handlers.

use bc::{generated, Interp};
use nh_runtime::{Ctx, SourceMap};

fn main() -> std::process::ExitCode {
    let path = std::env::args().nth(1).unwrap_or_else(|| "sample.bc".into());

    let mut sources = SourceMap::new();
    let file = match sources.load(&path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("cannot read `{path}`: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };

    let mut cx = Ctx::new(sources);
    let mut compiler = Interp::default();

    let outcome = generated::eval_source(&mut compiler, &mut cx, file);

    // Compiling produced instructions; running them produces output.
    eprintln!("--- bytecode ---");
    for (i, op) in compiler.code.iter().enumerate() {
        eprintln!("{i:3}  {op:?}");
    }
    eprintln!("--- output ---");
    for line in compiler.run() {
        println!("{line}");
    }

    match outcome {
        Ok(_) => std::process::ExitCode::SUCCESS,
        Err(errors) => {
            for d in &errors {
                eprint!("{}", d.render(cx.sources()));
                eprintln!();
            }
            std::process::ExitCode::FAILURE
        }
    }
}
