//! Thin binary over the library.
//!
//! Everything of interest is in `src/handlers/` — one small file per grammar
//! alternative, each reading its inputs by name.
//!
//! What is left here is only what belongs to *this program*: where the source
//! comes from, what to do with a result, and where errors go. Parse, syntax
//! check, build, and evaluate are `generated::eval_source`.

use config_example::{generated, Interp};
use nh_runtime::{Ctx, SourceMap};

fn main() -> std::process::ExitCode {
    let path = std::env::args().nth(1).unwrap_or_else(|| "sample.conf".into());

    let mut sources = SourceMap::new();
    let file = match sources.load(&path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("cannot read `{path}`: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };

    let mut cx = Ctx::new(sources);
    let mut interp = Interp;

    match generated::eval_source(&mut interp, &mut cx, file) {
        Ok(value) => {
            println!("{value}");
            std::process::ExitCode::SUCCESS
        }
        Err(errors) => {
            for d in &errors {
                eprint!("{}", d.render(cx.sources()));
                eprintln!();
            }
            std::process::ExitCode::FAILURE
        }
    }
}
