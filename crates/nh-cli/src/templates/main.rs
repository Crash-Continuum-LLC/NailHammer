//! Runs a {{Name}} program.
//!
//! Three things happen here, and all three are **yours**, which is why they are
//! in a file you own rather than in `src/generated/`:
//!
//!   1. Where the source comes from. A file here; a socket, a REPL line, or a
//!      string literal in a test somewhere else.
//!   2. What to do with the result.
//!   3. Where errors go, and what an exit code means.
//!
//! Everything in between — parse, report a parse error, collect syntax errors
//! that recovery got past, build the owned tree, evaluate — is
//! `generated::eval_source`. That sequence is the same in every project and has
//! exactly one correct order, so you are not asked to write it.

use nh_runtime::{Ctx, SourceMap};
use {{name}}::{generated, Interp};

{{tokiomain}}{{mainasync}}fn main() -> std::process::ExitCode {
    let path = std::env::args().nth(1).unwrap_or_else(|| "sample.{{ext}}".into());

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

    // Yours: what to do with what was produced, and whether it went wrong
    // *after* it compiled.
    //
    // This happens either way, on purpose. A run that recovered from a syntax
    // error still evaluated everything it could, and that output is worth
    // seeing — reporting the error is not a reason to hide it.
    let runtime_error: Option<String> = {
{{produced}}
    };

    match outcome {
        Ok(_) => match runtime_error {
            None => std::process::ExitCode::SUCCESS,
            Some(e) => {
                eprintln!("error: {e}");
                std::process::ExitCode::FAILURE
            }
        },

        // Yours: where errors go. `eval_source` returns them rather than
        // printing them, so a test, an LSP, and this binary can each do
        // something different with the same list.
        //
        // A program whose parse *recovered* arrives here too, even though
        // everything it could evaluate did — a reported typo is not a success.
        Err(errors) => {
            for d in &errors {
                eprint!("{}", d.render(cx.sources()));
                eprintln!();
            }
            std::process::ExitCode::FAILURE
        }
    }
}
