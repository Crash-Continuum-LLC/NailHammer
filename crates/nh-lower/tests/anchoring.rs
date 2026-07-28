//! Every shipped grammar accepts a program that opens with trivia.
//!
//! Pest skips whitespace *between* elements and never before the first one, so
//! an unanchored entry rule rejects any program starting with a blank line or a
//! comment. It parses everything else, which is what makes the mistake so easy
//! to keep: the grammar looks right and works on the first thing you try.
//!
//! This repository shipped two grammars with that defect — `example.nh` and
//! `examples/basic.nh`, both of which `USAGE.md` points readers at as models.
//! `nh check` cannot catch it, because nothing declares which rule is the entry
//! point. So it is checked here, against the real grammars.

use pest_vm::Vm;
use std::path::PathBuf;

/// `(grammar, entry rule, a program that opens with trivia)`.
///
/// Listed rather than discovered, because only a human knows which rule is the
/// entry point and what its language's comments look like.
/// `(grammar, entry rule, a program that opens with trivia)`.
///
/// Listed rather than discovered, because only a human knows which rule is the
/// entry point and what its language's comments look like.
///
/// **Each sample's first statement must begin with a literal or a token**, not
/// with an expression. An expression-led statement tolerates leading trivia by
/// accident: `expr` starts with `nh_pre_op*`, and pest inserts whitespace
/// skipping around a repetition, so `\n1 + 1;` parses even unanchored while
/// `\nlet a = 1;` does not. A sample built from the wrong one proves nothing.
const ENTRY_POINTS: &[(&str, &str, &str)] = &[
    ("example.nh", "program", "\n\nlet a = 1;\n"),
    ("examples/calc.nh", "program", "\n\nlet a = 1;\n"),
    // Line-oriented and with no `EOL*` before its first line, so a *blank* line
    // cannot parse there whatever the anchoring. Leading spaces are the trivia
    // this grammar can have, and they are enough to prove the point.
    ("examples/basic.nh", "program", "   PRINT 1\n"),
    ("examples/config/config.nh", "document", "\n# a comment\nname = 1;\n"),
    ("examples/calc-interp/calc.nh", "program", "\n# a comment\nlet a = 1;\n"),
    ("examples/basic-interp/basic.nh", "program", "\nREM a comment\nPRINT 1\n"),
    ("examples/bytecode/bc.nh", "program", "\n// a comment\nlet a = 1;\n"),
    ("examples/selfhost/nh.nh", "file", "\n// a comment\ngrammar G;\n"),
    // `nh init`'s template is not listed: it holds `{{name}}` placeholders and
    // is not valid `.nh` until scaffolded. `nh-cli`'s
    // `a_program_starting_with_trivia_parses` covers it after expansion, which
    // is the only form a user ever sees.
];

fn pest_of(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel);
    let mut sm = nh_syntax::SourceMap::new();
    let ast = nh_syntax::resolve(&mut sm, &path)
        .unwrap_or_else(|e| panic!("{rel}:\n{}", e.render(&sm)));
    let table = nh_operators::resolve(&ast, &mut sm)
        .unwrap_or_else(|e| panic!("{rel}:\n{}", e.render(&sm)));
    nh_lower::lower(&ast, &table)
        .unwrap_or_else(|e| panic!("{rel}:\n{}", e.render(&sm)))
        .pest
}

#[test]
fn every_entry_rule_accepts_leading_trivia() {
    for (grammar, entry, input) in ENTRY_POINTS {
        let pest = pest_of(grammar);
        let (_, rules) = pest_meta::parse_and_optimize(&pest)
            .unwrap_or_else(|e| panic!("{grammar}: {e:?}"));
        let vm = Vm::new(rules);

        assert!(
            vm.parse(entry, input).is_ok(),
            "`{grammar}` rejects a program that opens with a blank line or a \
             comment.\nhelp: anchor the entry rule — `rule {entry} = SOI .. EOI;`"
        );
    }
}

/// ...and the same program without the leading trivia, so a failure above is
/// definitely about anchoring rather than about the sample being wrong.
#[test]
fn the_same_programs_parse_without_leading_trivia() {
    for (grammar, entry, input) in ENTRY_POINTS {
        let pest = pest_of(grammar);
        let (_, rules) = pest_meta::parse_and_optimize(&pest).unwrap();
        let vm = Vm::new(rules);
        let trimmed = input.trim_start();

        assert!(
            vm.parse(entry, trimmed).is_ok(),
            "`{grammar}`: the sample program itself does not parse, so the \
             anchoring test above proves nothing"
        );
    }
}
