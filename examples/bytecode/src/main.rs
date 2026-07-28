//! Compiles a Bc program to bytecode, prints the instructions, then runs them.

use nh_runtime::{Ctx, SourceMap};
use pest::Parser;
use bc::{generated, Interp, Rule, BcParser};

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
    let text = sources.text(file).to_string();

    let mut pairs = match BcParser::parse(Rule::program, &text) {
        Ok(p) => p,
        Err(e) => {
            // `expect` declarations turn pest's rule names into a sentence.
            eprint!("{}", generated::render_parse_error(&e, file).render(&sources));
            return std::process::ExitCode::FAILURE;
        }
    };
    let program = pairs.next().expect("`program` yields one pair");

    // `recover` means the parse succeeded past bad statements, leaving error
    // nodes behind. Report them ALL before evaluating anything, so one typo
    // does not hide the rest.
    let syntax = generated::syntax_errors(&program, file);
    for d in &syntax {
        eprint!("{}", d.render(&sources));
        eprintln!();
    }

    let mut cx = Ctx::new(sources);
    cx.enter(nh_runtime::Span::new(file, 0, 0));
    let mut interp = Interp::default();

    // Two steps, and the first one is worth having on its own: `build_program`
    // produces an **owned** tree that outlives the parse, so it can be kept,
    // inspected, or run more than once.
    let result = generated::ast::build_program(program, file)
        .and_then(|tree| generated::dispatch::eval_program(&mut interp, &tree, &mut cx));

    // Compiling produced instructions; running them produces output.
    eprintln!("--- bytecode ---");
    for (i, op) in interp.code.iter().enumerate() {
        eprintln!("{i:3}  {op:?}");
    }
    eprintln!("--- output ---");
    for line in interp.run() {
        println!("{line}");
    }

    match result {
        Err(e) => {
            eprint!("{}", cx.render(&e));
            std::process::ExitCode::FAILURE
        }
        Ok(_) if !syntax.is_empty() => std::process::ExitCode::FAILURE,
        Ok(_) => std::process::ExitCode::SUCCESS,
    }
}
