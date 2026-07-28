//! Runs a BASIC program.

use basic_interp::{generated, BasicParser, Interp, Rule};
use nh_runtime::{Ctx, SourceMap};
use pest::Parser;

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
    let text = sources.text(file).to_string();

    let mut pairs = match BasicParser::parse(Rule::program, &text) {
        Ok(p) => p,
        Err(e) => {
            // The `expect` declaration turns pest's rule names into a sentence.
            let d = generated::render_parse_error(&e, file);
            eprint!("{}", d.render(&sources));
            return std::process::ExitCode::FAILURE;
        }
    };
    let program = pairs.next().expect("one program pair");

    // `recover` means the parse got past bad statements, leaving error nodes
    // behind. Report them all before evaluating anything.
    let syntax = generated::syntax_errors(&program, file);
    let recovered = syntax.len();
    for d in &syntax {
        eprint!("{}", d.render(&sources));
        eprintln!();
    }

    let mut cx = Ctx::new(sources);
    cx.enter(nh_runtime::Span::new(file, 0, 0));
    let mut interp = Interp::default();

    let outcome = match build_and_eval(&mut interp, program, file, &mut cx) {
        Ok(_) => {
            for line in &interp.output {
                println!("{line}");
            }
            std::process::ExitCode::SUCCESS
        }
        Err(e) => {
            // Whatever ran before the failure still produced output worth
            // seeing; printing it first is what makes a partial run useful.
            for line in &interp.output {
                println!("{line}");
            }
            eprint!("{}", cx.render(&e));
            std::process::ExitCode::FAILURE
        }
    };

    // A run that recovered from syntax errors is not a success, even if
    // everything it *could* evaluate did.
    if recovered > 0 {
        std::process::ExitCode::FAILURE
    } else {
        outcome
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
    let tree = generated::ast::build_program(pair, file)?;
    generated::dispatch::eval_program(interp, &tree, cx)
}
