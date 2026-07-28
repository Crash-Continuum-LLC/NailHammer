//! Runs a BASIC program.
//!
//! Only this program's own business is left here: where the source comes from,
//! what to do with output, and where errors go. Parse, syntax check, build, and
//! evaluate are `generated::eval_source`.

use basic_interp::{generated, Interp};
use nh_runtime::{Ctx, SourceMap};

fn main() -> std::process::ExitCode {
    let path = std::env::args().nth(1).unwrap_or_else(|| "sample.bas".into());

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

    // Whatever ran before a failure still produced output worth seeing, so this
    // happens either way. Printing it first is what makes a partial run useful.
    for line in &interp.output {
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
