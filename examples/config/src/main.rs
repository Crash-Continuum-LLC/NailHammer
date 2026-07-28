//! Thin binary over the library.
//!
//! Everything of interest is in `src/handlers/` — one small file per grammar
//! alternative, each reading its inputs by name.

use config_example::{generated, ConfigParser, Interp, Rule};
use nh_runtime::{Ctx, SourceMap};
use pest::Parser;

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

    let text = sources.text(file).to_string();
    let mut pairs = match ConfigParser::parse(Rule::document, &text) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let document = pairs.next().expect("`document` always yields one pair");

    let mut cx = Ctx::new(sources);
    // Seed the span stack so dispatch knows which file it is in.
    cx.enter(nh_runtime::Span::new(file, 0, 0));

    let mut interp = Interp;
    match build_and_eval(&mut interp, document, file, &mut cx) {
        Ok(value) => {
            println!("{value}");
            std::process::ExitCode::SUCCESS
        }
        Err(e) => {
            eprint!("{}", cx.render(&e));
            std::process::ExitCode::FAILURE
        }
    }
}

/// Builds the owned AST, then evaluates it.
///
/// Two steps rather than one because the tree is worth having: it outlives the
/// parse, so a caller can keep it, walk it, or run it more than once.
fn build_and_eval(
    interp: &mut Interp,
    pair: pest::iterators::Pair<'_, Rule>,
    file: nh_runtime::FileId,
    cx: &mut Ctx,
) -> nh_runtime::Result<<Interp as generated::dispatch::Semantics>::Out> {
    let tree = generated::ast::build_document(pair, file)?;
    generated::dispatch::eval_document(interp, &tree, cx)
}
