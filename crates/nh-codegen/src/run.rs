//! `generated/run.rs` — the part between "here is some source" and "the
//! handlers ran".
//!
//! **Why this is generated** (DESIGN §0). Every project used to hand-write the
//! same seven steps: parse, render a parse error, collect recovered syntax
//! errors, build a `Ctx`, build the tree, evaluate, decide an outcome. None of
//! it is a decision. All of it is easy to get subtly wrong, and it *was* wrong:
//! six of the eight parse sites in NailHammer's own examples and tests built a
//! tree without ever checking for recovered syntax errors, so a program with a
//! reported typo would quietly run anyway.
//!
//! **What is deliberately not here.** Loading the source is the caller's — a
//! project may read a file, a socket, or a string literal in a test. Formatting
//! a diagnostic is the caller's too, because where errors go is a property of
//! the program, not of the grammar. This returns them; `nh init` scaffolds the
//! loop that prints them.

use std::fmt::Write as _;

use nh_lower::Lowered;

use crate::{ident, type_name, Options, HEADER};

/// The rule a program starts at: the first one declared.
fn entry(lowered: &Lowered) -> Option<&str> {
    lowered.rules.first().map(|r| r.name.as_str())
}

pub fn generate(lowered: &Lowered, opts: &Options) -> String {
    let mut out = String::from(HEADER);

    let Some(entry) = entry(lowered) else {
        out.push_str("\n// This grammar declares no rules, so there is nothing to run.\n");
        return out;
    };

    // The entry rule has to *build* something, or the calls below name a
    // builder that was never generated.
    //
    // An unlabelled rule is an alias: it delegates to a single child and has no
    // node of its own. That is fine anywhere except here, and until this check
    // existed the failure was silent — `nh check --deny-warnings` passed,
    // `nh build` reported success, and the user's project failed to compile
    // with `cannot find function build_program`, pointing into generated code
    // they did not write. Saying it here puts the message in the file that
    // would otherwise be broken.
    if lowered
        .rules
        .first()
        .is_some_and(|r| matches!(r.shape, nh_lower::RuleShape::Alias { .. }))
    {
        let _ = write!(
            out,
            "\ncompile_error!(\n\
            \x20   \"the entry rule `{entry}` produces no node, so a program has nothing to \\\n\
            \x20    evaluate. Add a `-> label` to it -- `rule {entry} = SOI stmts:stmt* EOI -> doc;` \\\n\
            \x20    -- because the first rule declared is where a program starts, and a rule \\\n\
            \x20    with no label delegates to a single child rather than building anything.\"\n\
            );\n"
        );
        return out;
    }

    let e = ident(entry);
    let parser = &opts.parser_type;
    // `Rule` is emitted beside the parser type by `#[derive(Parser)]`.
    let rules = parser.rsplit_once("::").map(|(m, _)| m).unwrap_or("crate");
    let module = &opts.module_path;

    let _ = write!(
        out,
        r#"
//! Parse, check, build, evaluate — in the one order that is correct.
//!
//! Loading the source is yours. Deciding what an error looks like on screen is
//! yours. This is the part in between, which is the same in every project.

use nh_runtime::{{Ctx, Diagnostic, FileId, Span}};
use pest::Parser as _;

use {module}::{{ast, diagnostics, dispatch}};

/// Runs one source file that is already in `cx`'s [`SourceMap`].
///
/// ```ignore
/// let mut sources = SourceMap::new();
/// let file = sources.load(&path)?;          // yours
/// let mut cx = Ctx::new(sources);
///
/// match {module}::run::eval_source(&mut host, &mut cx, file) {{
///     Ok(value) => {{ /* .. */ }}
///     Err(errors) => for d in &errors {{     // yours
///         eprint!("{{}}", d.render(cx.sources()));
///     }},
/// }}
/// ```
///
/// `Ok` means the program parsed cleanly, evaluated, and reported nothing.
/// Anything else comes back as a list, in source order where that is knowable:
/// recovered syntax errors first, then whatever evaluation reported, then the
/// failure that stopped it.
///
/// **Recovered syntax errors are collected before evaluation and never
/// skipped.** `recover` lets a parse continue past a bad statement, leaving
/// error nodes behind; a tree that contains one is a tree that is known to be
/// wrong, so its result is never returned as `Ok`.
pub fn eval_source<H>(
    host: &mut H,
    cx: &mut Ctx,
    file: FileId,
) -> Result<<H as dispatch::Semantics>::Out, Vec<Diagnostic>>
where
    H: dispatch::Handlers,
{{
    let text = cx.sources().text(file).to_string();

    let mut pairs = match {parser}::parse({rules}::Rule::{entry}, &text) {{
        Ok(p) => p,
        // `expect` declarations turn pest's rule names into a sentence.
        Err(e) => return Err(vec![diagnostics::render_parse_error(&e, file)]),
    }};
    let node = pairs.next().expect("`{entry}` yields exactly one pair");

    // Before evaluating anything, so one typo does not hide the rest.
    let mut found = diagnostics::syntax_errors(&node, file);
    let reported_before = cx.diagnostics().len();

    // The whole file is the outermost span, so an error raised where nothing
    // narrower is in scope still points somewhere.
    cx.enter(Span::new(file, 0, 0));
    let outcome = ast::build_{e}(node, file)
        .and_then(|tree| dispatch::eval_{e}(host, &tree, cx));
    cx.leave();

    // Anything a handler reported through `cx.report` during this run. Taken by
    // index rather than wholesale, so a caller that reuses a `Ctx` across files
    // does not see the previous file's diagnostics again.
    //
    // Skipping duplicates is not tidiness. Evaluating an error node reports the
    // *same* syntax error `syntax_errors` already found, so without this every
    // reached recovery point is printed twice. `syntax_errors` is the one kept
    // because it is complete: it sees error nodes in code that never ran.
    for d in &cx.diagnostics()[reported_before..] {{
        if !found
            .iter()
            .any(|seen| seen.span == d.span && seen.message == d.message)
        {{
            found.push(d.clone());
        }}
    }}

    match outcome {{
        Ok(value) if found.is_empty() => Ok(value),
        // It evaluated, but the program was not clean. Returning the value here
        // would make a reported error look like a successful run.
        Ok(_) => Err(found),
        Err(e) => {{
            // `AlreadyReported` yields nothing: its diagnostic is above.
            found.extend(e.diagnostic());
            Err(found)
        }}
    }}
}}
"#
    );

    let _ = type_name; // kept for symmetry with the other emitters
    out
}
