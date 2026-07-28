//! Evaluates each statement in a file and prints the results.
//!
//! Three things happen here, and all three are this program's business rather
//! than the grammar's: where the source comes from, what to do with a result,
//! and where errors go. Everything in between — parse, check for recovered
//! syntax errors, build the owned tree, evaluate — is `generated::eval_source`.

use calc_interp::{generated, Interp};
use nh_runtime::{Ctx, SourceMap};

fn main() -> std::process::ExitCode {
    let path = std::env::args().nth(1).unwrap_or_else(|| "sample.calc".into());

    // Yours: where the source comes from.
    let mut sources = SourceMap::new();
    let file = match sources.load(&path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("cannot read `{path}`: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };

    let mut cx = Ctx::new(sources);
    let mut interp = Interp::default();

    let outcome = generated::eval_source(&mut interp, &mut cx, file);

    // Either way: a run that recovered from a syntax error still evaluated
    // everything it could, and that output is worth seeing.
    for line in &interp.output {
        println!("{line}");
    }

    match outcome {
        Ok(_) => std::process::ExitCode::SUCCESS,
        // Yours: where errors go. A recovered run arrives here even though
        // everything it *could* evaluate did — a reported typo is not success.
        Err(errors) => {
            for d in &errors {
                eprint!("{}", d.render(cx.sources()));
                eprintln!();
            }
            std::process::ExitCode::FAILURE
        }
    }
}
