//! M1 acceptance tests.
//!
//! The bar for this milestone is not "the emitter produced plausible text" but
//! **the generated grammar parses real programs**. `pest_vm` interprets a
//! grammar at runtime, so these tests run the actual pipeline —
//! `.nh` → `.pest` → parse — without a compile step in between.

use nh_lower::{lower, Lowered};
use nh_syntax::{resolve, SourceMap};
use pest_vm::Vm;
use std::path::{Path, PathBuf};

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(rel)
}

/// Lowers a grammar file, panicking with rendered diagnostics on failure.
fn build(path: &Path) -> Lowered {
    let mut sm = SourceMap::new();
    let ast = match resolve(&mut sm, path) {
        Ok(a) => a,
        Err(e) => panic!("parsing {} failed:\n{}", path.display(), e.render(&sm)),
    };
    let table = match nh_operators::resolve(&ast, &mut sm) {
        Ok(t) => t,
        Err(e) => panic!("operator table failed:\n{}", e.render(&sm)),
    };
    match lower(&ast, &table) {
        Ok(l) => l,
        Err(e) => panic!("lowering {} failed:\n{}", path.display(), e.render(&sm)),
    }
}

fn build_str(source: &str) -> Lowered {
    let dir = std::env::temp_dir().join("nh-lower-tests");
    std::fs::create_dir_all(&dir).unwrap();
    let path = unique_path(&dir, ".nh");
    std::fs::write(&path, source).unwrap();
    build(&path)
}

/// A path no other test can be writing.
///
/// This used to be a content hash, with a comment claiming it stopped
/// collisions. It caused them: two tests with *identical* grammar text got the
/// same path, and one truncated the file while the other was reading it. The
/// symptom was an occasional "no `grammar` declaration found" in whichever test
/// lost the race.
fn unique_path(dir: &std::path::Path, ext: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    dir.join(format!(
        "g{}_{}{ext}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ))
}


/// Compiles generated pest source into a runnable VM.
fn vm(pest: &str) -> Vm {
    match pest_meta::parse_and_optimize(pest) {
        Ok((_, rules)) => Vm::new(rules),
        Err(errors) => {
            let msgs: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
            panic!(
                "generated grammar is not valid pest:\n{}\n--- grammar ---\n{pest}",
                msgs.join("\n")
            )
        }
    }
}

/// Parses `input` with `rule`, returning the flattened rule names it produced.
fn parse(vm: &Vm, rule: &str, input: &str) -> Result<Vec<String>, String> {
    match vm.parse(rule, input) {
        Ok(pairs) => Ok(pairs
            .flatten()
            .map(|p| p.as_rule().to_string())
            .collect()),
        Err(e) => Err(e.to_string()),
    }
}

// ---------------------------------------------------------------------------
// The shipped examples produce valid pest
// ---------------------------------------------------------------------------

#[test]
fn all_examples_lower_to_valid_pest() {
    for path in ["example.nh", "examples/calc.nh", "examples/basic.nh"] {
        let lowered = build(&repo(path));
        // Panics with the offending grammar if pest rejects it.
        let _ = vm(&lowered.pest);
    }
}

// ---------------------------------------------------------------------------
// Parsing real programs
// ---------------------------------------------------------------------------

#[test]
fn example_grammar_parses_a_program() {
    let lowered = build(&repo("example.nh"));
    let vm = vm(&lowered.pest);

    let rules = parse(&vm, "program", "let x = 1 + 2 * 3;\nx * (4 - 1);\n")
        .unwrap_or_else(|e| panic!("{e}"));

    assert!(rules.contains(&"stmt_let".to_string()), "{rules:?}");
    assert!(rules.contains(&"stmt_eval".to_string()), "{rules:?}");
    assert!(rules.contains(&"primary_num".to_string()), "{rules:?}");
    assert!(rules.contains(&"primary_var".to_string()), "{rules:?}");
}

#[test]
fn calc_grammar_parses_a_program() {
    let lowered = build(&repo("examples/calc.nh"));
    let vm = vm(&lowered.pest);

    // No leading newline: `program` is anchored with SOI, and implicit
    // skipping applies between elements, not before the first one.
    let src = r#"let greeting = "hi";
let n = obj.field[0] + f(1, 2) * 3;
if n { return n; } else { return 0; }
n |> f;
"#;
    let rules = parse(&vm, "program", src).unwrap_or_else(|e| panic!("{e}"));

    // The suffix chain (DESIGN.md §6.7) is grammar, not operators.
    assert!(rules.contains(&"suffix_field".to_string()), "{rules:?}");
    assert!(rules.contains(&"suffix_index".to_string()), "{rules:?}");
    assert!(rules.contains(&"suffix_call".to_string()), "{rules:?}");
    // `|>` was added by `precedence override`.
    assert!(rules.contains(&"nh_op_pipe_gt".to_string()), "{rules:?}");
}

#[test]
fn basic_grammar_parses_a_program() {
    let lowered = build(&repo("examples/basic.nh"));
    let vm = vm(&lowered.pest);

    // Word operators, case folding, and a from-scratch table all at once.
    let src = "10 LET Counter = 1\n20 IF counter < 10 AND NOT done THEN PRINT \"hi\"\n30 END\n";
    let rules = parse(&vm, "program", src).unwrap_or_else(|e| panic!("{e}"));

    assert!(rules.contains(&"stmt_let".to_string()), "{rules:?}");
    assert!(rules.contains(&"stmt_if".to_string()), "{rules:?}");
    assert!(rules.contains(&"nh_op_and".to_string()), "{rules:?}");
    assert!(rules.contains(&"nh_op_not".to_string()), "{rules:?}");
}

// ---------------------------------------------------------------------------
// Keyword and word-operator boundary guards (DESIGN.md §5.3, §6.5)
// ---------------------------------------------------------------------------

const GUARD_GRAMMAR: &str = r#"
grammar Guard;
use operators::none;
skip WS = " ";
token ALPHA = @ "a".."z" | "A".."Z";
token DIGIT = @ "0".."9";
token IDENT = @ ALPHA (ALPHA | DIGIT | "_")*;
reserved from IDENT { "let" }
rule atom = primary;
rule primary = name:IDENT -> var;
rule stmt = "let" name:IDENT -> let | name:IDENT -> bare;
"#;

#[test]
fn keyword_does_not_match_inside_a_longer_identifier() {
    let lowered = build_str(GUARD_GRAMMAR);
    let vm = vm(&lowered.pest);

    // `let x` is a let-statement.
    let rules = parse(&vm, "stmt", "let x").unwrap();
    assert!(rules.contains(&"stmt_let".to_string()), "{rules:?}");

    // `letter` is a *bare identifier*, not `let` followed by `ter`. Without the
    // boundary guard the first alternative would win and consume `let`.
    let rules = parse(&vm, "stmt", "letter").unwrap();
    assert!(
        rules.contains(&"stmt_bare".to_string()),
        "`letter` must not lex as `let` + `ter`: {rules:?}"
    );
}

#[test]
fn reserved_words_are_rejected_by_the_identifier_token() {
    let lowered = build_str(GUARD_GRAMMAR);
    let vm = vm(&lowered.pest);

    // `let` alone cannot be an identifier, so `bare` cannot match it.
    assert!(
        parse(&vm, "primary", "let").is_err(),
        "`let` must not parse as an identifier"
    );
    assert!(parse(&vm, "primary", "letter").is_ok());
}

#[test]
fn word_operators_are_boundary_guarded_and_auto_reserved() {
    let lowered = build(&repo("examples/basic.nh"));
    let vm = vm(&lowered.pest);

    // `ANDY` is an identifier, not `AND` followed by `Y`.
    let rules = parse(&vm, "expr", "ANDY").unwrap_or_else(|e| panic!("{e}"));
    assert!(
        !rules.iter().any(|r| r == "nh_op_and"),
        "`ANDY` must not lex as the AND operator: {rules:?}"
    );

    // ...and `AND` itself is reserved, so it cannot be a variable name, even
    // though basic.nh never lists it in `reserved from` (DESIGN.md §6.5).
    assert!(
        parse(&vm, "primary", "AND").is_err(),
        "word operators are auto-reserved"
    );
}

// ---------------------------------------------------------------------------
// Operator alternation ordering (DESIGN.md §5.2)
// ---------------------------------------------------------------------------

#[test]
fn longer_operators_win_over_their_prefixes() {
    let lowered = build(&repo("examples/calc.nh"));
    let vm = vm(&lowered.pest);

    // If `<` were tried before `<=`, this would parse as `a < (= b)` and fail.
    for src in ["a <= b", "a >= b", "a == b", "a != b", "a << b", "a && b", "a || b"] {
        parse(&vm, "expr", src).unwrap_or_else(|e| panic!("`{src}` should parse:\n{e}"));
    }
}

#[test]
fn operator_alternation_is_sorted_longest_first() {
    let lowered = build(&repo("examples/calc.nh"));
    let line = lowered
        .pest
        .lines()
        .find(|l| l.starts_with("nh_bin_op"))
        .expect("a binary operator alternation");

    // Extract each operator rule's literal from the emitted definitions, in the
    // order they appear in the alternation, and assert lengths are descending.
    let order: Vec<usize> = line
        .split('|')
        .filter_map(|part| {
            let name = part.trim().trim_start_matches("nh_bin_op = _{").trim();
            let name = name.trim_end_matches('}').trim();
            lowered
                .pest
                .lines()
                .find(|l| l.starts_with(&format!("{name} = @{{")))
                .and_then(|def| def.split('"').nth(1).map(str::len))
        })
        .collect();

    assert!(order.len() > 5, "expected several operators, got {order:?}");
    assert!(
        order.windows(2).all(|w| w[0] >= w[1]),
        "operator alternation is not longest-first: {order:?}"
    );
}

// ---------------------------------------------------------------------------
// Bindings become node tags (DESIGN.md §2)
// ---------------------------------------------------------------------------

#[test]
fn bindings_are_emitted_as_node_tags() {
    let lowered = build(&repo("example.nh"));
    assert!(
        lowered.pest.contains("#name = IDENT"),
        "binding did not become a tag:\n{}",
        lowered.pest
    );
    assert!(lowered.pest.contains("#value = expr"), "{}", lowered.pest);
}

#[test]
fn lowered_alternatives_record_their_bindings_for_m2() {
    let lowered = build(&repo("example.nh"));
    let let_stmt = lowered
        .alternatives
        .iter()
        .find(|a| a.label == "let")
        .expect("the let_stmt alternative");

    assert_eq!(let_stmt.rule, "stmt");
    assert_eq!(let_stmt.pest_rule, "stmt_let");
    let names: Vec<&str> = let_stmt.bindings.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(names, vec!["name", "value"]);
    // `name:IDENT` is a required binding onto a token.
    assert_eq!(let_stmt.bindings[0].cardinality, nh_lower::Cardinality::One);
    assert_eq!(
        let_stmt.bindings[0].token.as_ref().map(|t| t.name.as_str()),
        Some("IDENT")
    );

    let var = lowered
        .alternatives
        .iter()
        .find(|a| a.label == "var")
        .expect("the var alternative");
    assert!(var.place, "`-> var place` must be recorded as assignable");
}

// ---------------------------------------------------------------------------
// Case folding (DESIGN.md §5.3)
// ---------------------------------------------------------------------------

#[test]
fn keywords_fold_case_when_declared() {
    let lowered = build(&repo("examples/basic.nh"));
    let vm = vm(&lowered.pest);

    for src in ["10 PRINT \"x\"\n", "10 print \"x\"\n", "10 PrInT \"x\"\n"] {
        parse(&vm, "program", src).unwrap_or_else(|e| panic!("`{src}` should parse:\n{e}"));
    }
}

#[test]
fn keywords_do_not_fold_by_default() {
    let lowered = build(&repo("example.nh"));
    let vm = vm(&lowered.pest);

    parse(&vm, "program", "let x = 1;").expect("lowercase `let` parses");
    assert!(
        parse(&vm, "program", "LET x = 1;").is_err(),
        "`LET` must not match `let` without `keywords case-insensitive`"
    );
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

fn lower_err(source: &str) -> String {
    let dir = std::env::temp_dir().join("nh-lower-tests");
    std::fs::create_dir_all(&dir).unwrap();
    let path = unique_path(&dir, ".nh");
    std::fs::write(&path, source).unwrap();

    let mut sm = SourceMap::new();
    let ast = resolve(&mut sm, &path).unwrap_or_else(|e| panic!("{}", e.render(&sm)));
    let table = match nh_operators::resolve(&ast, &mut sm) {
        Ok(t) => t,
        Err(e) => return e.render(&sm),
    };
    match lower(&ast, &table) {
        Ok(_) => panic!("expected lowering to fail"),
        Err(e) => e.render(&sm),
    }
}

#[test]
fn undefined_reference_is_reported() {
    let out = lower_err("grammar A;\nrule r = missing_thing;\n");
    assert!(out.contains("undefined reference `missing_thing`"), "{out}");
}

#[test]
fn expr_without_an_operator_table_explains_itself() {
    let out = lower_err("grammar A;\nrule r = expr;\n");
    assert!(out.contains("undefined reference `expr`"), "{out}");
    assert!(out.contains("operator system"), "{out}");
}

#[test]
fn unknown_preset_lists_the_available_ones() {
    let out = lower_err("grammar A;\nuse operators::klingon;\nrule atom = \"x\";\n");
    assert!(out.contains("unknown operator preset `klingon`"), "{out}");
    assert!(out.contains("c_style"), "{out}");
}

#[test]
fn removing_an_absent_operator_is_an_error() {
    let out = lower_err(
        "grammar A;\nuse operators::core;\nprecedence override { remove \"@@\"; }\nrule atom = \"x\";\n",
    );
    assert!(out.contains("cannot remove `@@`"), "{out}");
}

#[test]
fn operator_without_a_known_role_demands_a_binding() {
    let out = lower_err(
        "grammar A;\nuse operators::none;\nprecedence override { left \"|>\"; }\nrule atom = \"x\";\n",
    );
    assert!(out.contains("no built-in role for operator `|>`"), "{out}");
    assert!(out.contains("-> pipe") || out.contains("explicit binding"), "{out}");
}


// ---------------------------------------------------------------------------
// Binding cardinality (drives view accessor shapes at M2)
// ---------------------------------------------------------------------------

fn binding<'a>(l: &'a Lowered, label: &str, name: &str) -> &'a nh_lower::Binding {
    l.alternatives
        .iter()
        .find(|a| a.label == label)
        .unwrap_or_else(|| panic!("no alternative labelled `{label}`"))
        .bindings
        .iter()
        .find(|b| b.name == name)
        .unwrap_or_else(|| panic!("no binding `{name}` on `{label}`"))
}

#[test]
fn cardinality_is_derived_from_the_enclosing_structure() {
    use nh_lower::Cardinality;
    let l = build(&repo("examples/calc.nh"));

    // Plain sequence element.
    assert_eq!(binding(&l, "let", "name").cardinality, Cardinality::One);
    // Under `?`.
    assert_eq!(binding(&l, "if", "tail").cardinality, Cardinality::Optional);
    assert_eq!(binding(&l, "call", "args").cardinality, Cardinality::Optional);
    // Under `*`. (`-> pass` alternatives get no rule and no view, so this uses
    // a grammar where the repeated binding carries a real label.)
    let m = build_str(
        "grammar M;\nskip WS = \" \";\ntoken ALPHA = @ \"a\"..\"z\";\n\
         token IDENT = @ ALPHA+;\nrule r = items:IDENT* -> many;\n",
    );
    assert_eq!(binding(&m, "many", "items").cardinality, Cardinality::Many);
}

#[test]
fn case_insensitive_tokens_are_flagged_on_bindings() {
    // BASIC folds identifiers, so `name:IDENT` must expose `.key()`.
    let l = build(&repo("examples/basic.nh"));
    let b = binding(&l, "var", "name");
    let token = b.token.as_ref().expect("bound to a token");
    assert_eq!(token.name, "IDENT");
    assert!(token.case_insensitive, "BASIC folds identifiers");

    // calc.nh does not fold, so the same binding must NOT offer `.key()`.
    let l = build(&repo("examples/calc.nh"));
    let token = binding(&l, "var", "name").token.as_ref().unwrap();
    assert!(!token.case_insensitive);
}

/// Regression: a repeated binding must tag **every** iteration.
///
/// Pest's grammar places the postfix operator inside a tagged term, so
/// `#items = value*` tags the repetition rather than each match — and the first
/// iteration comes back untagged. A view built on that silently drops the first
/// element of every list, which is exactly how it was found: a config file's
/// first key vanished from the parsed output.
#[test]
fn repeated_bindings_tag_every_iteration() {
    let l = build(&repo("examples/config/config.nh"));
    assert!(
        l.pest.contains("(#entries = entry)*"),
        "the tag must be inside the repetition:\n{}",
        l.pest
    );
    assert!(
        !l.pest.contains("#entries = entry*"),
        "tagging the repetition itself drops the first element:\n{}",
        l.pest
    );

    let vm = vm(&l.pest);
    let pairs = vm
        .parse("document", "a = 1; b = 2; c = 3;")
        .unwrap_or_else(|e| panic!("{e}"));

    let tagged = pairs
        .flatten()
        .filter(|p| p.as_node_tag() == Some("entries"))
        .count();
    assert_eq!(tagged, 3, "every entry must carry the `entries` tag");
}

// ---------------------------------------------------------------------------
// `expect` targeting (DESIGN.md §5.5)
// ---------------------------------------------------------------------------

const TWO_EXPECTS: &str = r#"
grammar E;
skip WS = " ";
token ALPHA = @ "a".."z";
token IDENT = @ ALPHA+;
rule call  = name:IDENT "(" args:IDENT ")" -> call;
rule group = "(" inner:IDENT ")" -> group;
rule top = call | group;
expect ")" in call  as "closing parenthesis of call arguments";
expect ")" in group as "closing parenthesis of group";
"#;

/// The same literal can carry different messages in different rules.
///
/// Keying expectations on the literal alone made the second declaration
/// silently vanish — the `in <rule>` clause was parsed, carried through the
/// AST, and never read.
#[test]
fn two_expects_for_one_literal_stay_distinct() {
    let l = build_str(TWO_EXPECTS);

    assert!(l.pest.contains("nh_expect_call_rparen"), "{}", l.pest);
    assert!(l.pest.contains("nh_expect_group_rparen"), "{}", l.pest);

    let messages: Vec<&str> = l.expectations.iter().map(|(_, m)| m.as_str()).collect();
    assert!(messages.contains(&"closing parenthesis of call arguments"), "{messages:?}");
    assert!(messages.contains(&"closing parenthesis of group"), "{messages:?}");
}

/// An expectation applies only inside its target rule.
#[test]
fn an_expectation_does_not_leak_into_other_rules() {
    let l = build_str(TWO_EXPECTS);
    let call = l
        .pest
        .lines()
        .find(|line| line.starts_with("call ="))
        .expect("the call rule");
    assert!(call.contains("nh_expect_call_rparen"), "{call}");
    assert!(!call.contains("nh_expect_group_rparen"), "{call}");
}

/// `rule.label` scopes to one alternative, not the whole rule.
#[test]
fn a_labelled_target_scopes_to_that_alternative() {
    let l = build(&repo("examples/calc.nh"));
    let suffix_call = l
        .pest
        .lines()
        .find(|line| line.starts_with("suffix_call ="))
        .expect("the suffix_call rule");
    assert!(suffix_call.contains("nh_expect_suffix_call_lparen"), "{suffix_call}");

    // `suffix_index` uses `[`, and must not have picked up the `(` expectation.
    let suffix_index = l
        .pest
        .lines()
        .find(|line| line.starts_with("suffix_index ="))
        .expect("the suffix_index rule");
    assert!(!suffix_index.contains("nh_expect"), "{suffix_index}");
}

/// An `expect` naming a rule that does not exist silences nothing and the
/// author believes they are covered — the same failure mode as an unknown lint.
#[test]
fn an_expect_on_an_unknown_target_is_an_error() {
    let out = lower_err(
        "grammar A;\nskip WS = \" \";\nrule r = \"x\" -> lit;\n\
         expect \"x\" in nonexistent as \"thing\";\n",
    );
    assert!(out.contains("unknown target `nonexistent`"), "{out}");
}

#[test]
fn an_expect_on_an_unknown_label_is_an_error() {
    let out = lower_err(
        "grammar A;\nskip WS = \" \";\nrule r = \"x\" -> lit;\n\
         expect \"x\" in r.nope as \"thing\";\n",
    );
    assert!(out.contains("unknown target `r.nope`"), "{out}");
}

#[test]
fn two_expects_for_the_same_literal_and_target_conflict() {
    let out = lower_err(
        "grammar A;\nskip WS = \" \";\nrule r = \"x\" -> lit;\n\
         expect \"x\" in r as \"one\";\nexpect \"x\" in r as \"two\";\n",
    );
    assert!(out.contains("already has an `expect` message"), "{out}");
}

// ---------------------------------------------------------------------------
// `guard from` — boundary-guard without reserving (DESIGN.md §11)
// ---------------------------------------------------------------------------

const GUARDED: &str = r#"
grammar G;
skip WS = " ";
token ALPHA = @ "a".."z";
token DIGIT = @ "0".."9";
token IDENT = @ ALPHA (ALPHA | DIGIT)*;
guard from IDENT { "atom" }
rule atom = name:IDENT -> var;
rule top = "atom" body:atom -> t | name:IDENT -> bare;
"#;

/// The half of `reserved from` that a contextual keyword needs: guard the
/// literal, but leave it usable as an identifier.
#[test]
fn a_guarded_word_is_still_a_valid_identifier() {
    let l = build_str(GUARDED);
    let vm = vm(&l.pest);

    // Guarded: `atomic` is one identifier, not `atom` + `ic`.
    let rules = parse(&vm, "top", "atomic").unwrap();
    assert!(
        rules.contains(&"top_bare".to_string()),
        "`atomic` must not lex as `atom` + `ic`: {rules:?}"
    );

    // Not reserved: `atom` is still an identifier where one is expected.
    parse(&vm, "atom", "atom").expect("a guarded word is still an identifier");
}

/// The contrast that motivates the feature.
#[test]
fn a_reserved_word_is_rejected_where_a_guarded_one_is_not() {
    let reserved = build_str(&GUARDED.replace("guard from", "reserved from"));
    assert!(
        parse(&vm(&reserved.pest), "atom", "atom").is_err(),
        "`reserved from` forbids it as an identifier"
    );

    let guarded = build_str(GUARDED);
    assert!(
        parse(&vm(&guarded.pest), "atom", "atom").is_ok(),
        "`guard from` does not"
    );
}

/// Only reserved words feed the identifier token's rejection set.
#[test]
fn guarding_does_not_touch_the_identifier_token() {
    let l = build_str(GUARDED);
    let ident = l
        .pest
        .lines()
        .find(|line| line.starts_with("IDENT ="))
        .expect("the IDENT token");
    assert!(
        !ident.contains("nh_reserved"),
        "a guard must not add a rejection to the token:\n{ident}"
    );
    assert!(l.pest.contains("nh_kw_atom"), "but the literal is guarded:\n{}", l.pest);
}

#[test]
fn guard_from_an_unknown_token_is_an_error() {
    let out = lower_err(
        "grammar A;\nskip WS = \" \";\ntoken ALPHA = @ \"a\"..\"z\";\n\
         guard from NOPE { \"x\" }\nrule r = \"x\" -> lit;\n",
    );
    assert!(out.contains("`guard from` names unknown token `NOPE`"), "{out}");
}

/// Word operators still auto-reserve, and now find their identifier token
/// through a `guard from` declaration when there is no `reserved from`.
#[test]
fn word_operators_work_with_only_a_guard_declaration() {
    let l = build_str(
        "grammar W;\nuse operators::none;\nskip WS = \" \";\n\
         token ALPHA = @ \"a\"..\"z\" | \"A\"..\"Z\";\ntoken IDENT = @ ALPHA+;\n\
         guard from IDENT { \"let\" }\n\
         precedence { left word \"AND\" -> bit_and; atom primary; }\n\
         rule primary = name:IDENT -> var;\n",
    );
    let vm = vm(&l.pest);

    // `ANDY` is an identifier, not `AND` + `Y`.
    let rules = parse(&vm, "expr", "ANDY").unwrap();
    assert!(!rules.iter().any(|r| r == "nh_op_and"), "{rules:?}");
    // ...and `AND` itself is reserved, because a word operator cannot also be
    // a variable name.
    assert!(parse(&vm, "primary", "AND").is_err());
}

// ---------------------------------------------------------------------------
// `boundary` — stating a token's continuation class (DESIGN.md §11)
// ---------------------------------------------------------------------------

/// A token with no repeated tail cannot have its boundary derived precisely.
/// Being quietly approximate there was the complaint; now it says so.
#[test]
fn an_underivable_boundary_is_reported() {
    let l = build_str(
        "grammar A;\nskip WS = \" \";\ntoken ALPHA = @ \"a\"..\"z\";\n\
         token TWO = @ ALPHA ALPHA;\nreserved from TWO { \"ab\" }\n\
         rule r = \"ab\" -> lit | name:TWO -> v;\n",
    );
    let messages: Vec<&str> = l.diagnostics.iter().map(|d| d.message.as_str()).collect();
    assert!(
        messages.iter().any(|m| m.contains("cannot derive an identifier boundary")),
        "{messages:?}"
    );
}

#[test]
fn a_boundary_declaration_silences_it_and_is_used() {
    let l = build_str(
        "grammar A;\nskip WS = \" \";\ntoken ALPHA = @ \"a\"..\"z\";\n\
         token TWO = @ ALPHA ALPHA;\nboundary TWO = ALPHA;\n\
         reserved from TWO { \"ab\" }\nrule r = \"ab\" -> lit | name:TWO -> v;\n",
    );
    assert!(l.diagnostics.is_empty(), "{:?}", l.diagnostics);
    assert!(
        l.pest.contains("nh_cont_TWO = _{ ALPHA }"),
        "the declared boundary should be what the guard uses:\n{}",
        l.pest
    );
}

/// An ordinary identifier token derives precisely, so no warning.
#[test]
fn a_normal_identifier_derives_precisely() {
    let l = build_str(
        "grammar A;\nskip WS = \" \";\ntoken ALPHA = @ \"a\"..\"z\";\n\
         token DIGIT = @ \"0\"..\"9\";\ntoken IDENT = @ ALPHA (ALPHA | DIGIT)*;\n\
         reserved from IDENT { \"let\" }\nrule r = \"let\" -> lit | name:IDENT -> v;\n",
    );
    assert!(l.diagnostics.is_empty(), "{:?}", l.diagnostics);
}

/// The declared boundary must actually guard: `abc` is one token, not `ab` + `c`.
#[test]
fn a_declared_boundary_guards_correctly() {
    let l = build_str(
        "grammar A;\nskip WS = \" \";\ntoken ALPHA = @ \"a\"..\"z\";\n\
         token TWO = @ ALPHA ALPHA;\nboundary TWO = ALPHA;\n\
         guard from TWO { \"ab\" }\nrule r = \"ab\" x:TWO -> pair | name:TWO -> v;\n",
    );
    let vm = vm(&l.pest);
    // `abcd` -> the leading `ab` is followed by `c`, which continues a TWO, so
    // the keyword must not match and the bare-token alternative wins.
    let rules = parse(&vm, "r", "abcd").unwrap();
    assert!(rules.contains(&"r_v".to_string()), "{rules:?}");
}

// ---------------------------------------------------------------------------
// Token atomicity (DESIGN.md §11)
// ---------------------------------------------------------------------------

const TOKENS: &str = r#"
grammar A;
skip WS = " ";
token ALPHA   = @ "a".."z";
token INNER   = @ ALPHA+;
token WRAPPED = "<" INNER ">";
rule r = w:WRAPPED -> v;
"#;

/// A `token` never skips whitespace, with or without `@`.
///
/// A plain `{ }` would let `< abc >` match a *token*, which is never what
/// `token` means. Non-atomic tokens are compound-atomic.
#[test]
fn a_token_never_skips_whitespace() {
    let l = build_str(TOKENS);
    assert!(l.pest.contains("WRAPPED = ${"), "{}", l.pest);

    let vm = vm(&l.pest);
    parse(&vm, "WRAPPED", "<abc>").expect("the token itself matches");
    assert!(
        parse(&vm, "WRAPPED", "< abc >").is_err(),
        "whitespace must not be skipped inside a token"
    );
}

/// Compound-atomic, not atomic: inner rules still produce nodes. That is the
/// distinction `@` gives up, and the one `nh.pest` needs for string literals.
#[test]
fn a_non_atomic_token_keeps_its_inner_nodes() {
    let l = build_str(TOKENS);
    let vm = vm(&l.pest);
    let nodes = parse(&vm, "WRAPPED", "<abc>").unwrap();
    assert!(nodes.contains(&"INNER".to_string()), "{nodes:?}");
}

/// `@` suppresses them, which is what it is for.
#[test]
fn an_atomic_token_hides_its_inner_nodes() {
    let l = build_str(&TOKENS.replace(
        "token WRAPPED = \"<\" INNER \">\";",
        "token WRAPPED = @ \"<\" INNER \">\";",
    ));
    assert!(l.pest.contains("WRAPPED = @{"), "{}", l.pest);

    let vm = vm(&l.pest);
    let nodes = parse(&vm, "WRAPPED", "<abc>").unwrap();
    assert!(!nodes.contains(&"INNER".to_string()), "{nodes:?}");
}

// ---------------------------------------------------------------------------
// Rule shapes (M7: the owned AST)
// ---------------------------------------------------------------------------

const SHAPES: &str = r#"
grammar S;
use operators::core;
skip WS = " ";
token DIGIT = @ "0".."9";
token ALPHA = @ "a".."z";
token NUMBER = @ DIGIT+;
token IDENT = @ ALPHA+;
rule program = SOI stmts:stmt* EOI -> doc;
rule stmt
  = "let" name:IDENT "=" value:expr ";" -> bind
  | value:expr ";"                      -> eval
  ;
rule atom = primary;
rule primary
  = digits:NUMBER      -> num
  | "(" inner:expr ")" -> pass
  ;
"#;

fn shape_of<'a>(l: &'a Lowered, rule: &str) -> &'a nh_lower::RuleShape {
    &l.rules
        .iter()
        .find(|r| r.name == rule)
        .unwrap_or_else(|| panic!("no rule `{rule}`"))
        .shape
}

/// A rule with one labelled alternative is one struct, not an enum wrapping
/// one struct — the same collapse the handler names already use.
#[test]
fn a_single_alternative_rule_is_one_node() {
    let l = build_str(SHAPES);
    assert!(
        matches!(shape_of(&l, "program"), nh_lower::RuleShape::Single { pest_rule } if pest_rule == "program"),
        "{:?}",
        shape_of(&l, "program")
    );
}

#[test]
fn a_multi_alternative_rule_is_a_choice_of_its_labels() {
    let l = build_str(SHAPES);
    let nh_lower::RuleShape::Choice(vs) = shape_of(&l, "stmt") else {
        panic!("{:?}", shape_of(&l, "stmt"));
    };
    let labels: Vec<&str> = vs
        .iter()
        .filter_map(|v| match v {
            nh_lower::LoweredVariant::Labelled { label, .. } => Some(label.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(labels, vec!["bind", "eval"]);
}

/// `rule atom = primary;` carries no node of its own. Generating a type for it
/// would put a wrapper in every handler signature that mentions an atom.
#[test]
fn a_plain_alias_is_not_a_node() {
    let l = build_str(SHAPES);
    assert!(
        matches!(
            shape_of(&l, "atom"),
            nh_lower::RuleShape::Alias { child } if child.as_deref() == Some("primary")
        ),
        "{:?}",
        shape_of(&l, "atom")
    );
}

/// A `-> pass` alternative yields whatever its one rule reference does, and
/// the AST needs to name that type. `"(" inner:expr ")"` yields an `expr` —
/// the literals contribute no node.
#[test]
fn a_pass_alternative_names_the_rule_it_yields() {
    let l = build_str(SHAPES);
    let nh_lower::RuleShape::Choice(vs) = shape_of(&l, "primary") else {
        panic!("{:?}", shape_of(&l, "primary"));
    };
    let transparent: Vec<Option<&str>> = vs
        .iter()
        .filter_map(|v| match v {
            nh_lower::LoweredVariant::Transparent { child } => Some(child.as_deref()),
            _ => None,
        })
        .collect();
    assert_eq!(transparent, vec![Some("expr")], "{vs:?}");
}

// ---------------------------------------------------------------------------
// Transparent alternatives
//
// An alternative with no label — or `-> pass` — has no node of its own, so
// something else has to be the node it stands for. That works only when the
// body produces exactly one pest pair belonging to a rule.
//
// Getting this wrong used to be found *late*, and in two different places:
// generated Rust that would not compile, or a parse that failed at run time.
// These pin both to `nh check`.
// ---------------------------------------------------------------------------

/// The prelude every case below shares: two tokens and an operator-free table.
const T: &str = "grammar T;\n\
                 use operators::none;\n\
                 skip WS = \" \" | \"\\t\";\n\
                 token EOL = @ \"\\n\";\n\
                 token ALPHA = @ \"a\"..\"z\";\n\
                 token ID = @ ALPHA+;\n";

fn transparent_err(rules: &str) -> String {
    lower_err(&format!("{T}{rules}"))
}

fn transparent_ok(rules: &str) {
    build_str(&format!("{T}{rules}"));
}

/// A token is a node too.
///
/// This is the bug that started it: counting *rule* references found one
/// (`stmt`) and called the alternative transparent. `EOL` is a token, and a
/// token produces a pair — so the wrapper pair reached `build_stmt`, which
/// looks for its tags among direct children and found none. `nh check` passed,
/// `nh build` passed, and it failed while parsing.
#[test]
fn a_token_counts_as_a_node() {
    let out = transparent_err(
        "rule program = SOI lines:line+ EOI -> prog;\n\
         rule line = body:stmt EOL+ -> pass;\n\
         rule stmt = v:ID -> s;\n",
    );
    assert!(out.contains("`-> pass`"), "{out}");
    assert!(out.contains("produces 2 or more nodes"), "{out}");
    assert!(out.contains("give this one a `-> label`"), "{out}");
}

/// Repetition widens the count rather than being ignored.
///
/// The old check walked through `*` and saw one rule reference, so `stmt*`
/// looked like a single child. It is any number of them.
#[test]
fn a_repetition_is_not_one_node() {
    let out = transparent_err(
        "rule program = body:many -> prog;\n\
         rule many = stmt*;\n\
         rule stmt = v:ID -> s;\n",
    );
    assert!(out.contains("no label"), "{out}");
    assert!(out.contains("produces 0 or more nodes"), "{out}");
}

/// A fixed count above one is reported exactly, not as "or more".
#[test]
fn two_rules_are_two_nodes() {
    let out = transparent_err(
        "rule program = body:pair -> prog;\n\
         rule pair = stmt stmt;\n\
         rule stmt = v:ID -> s;\n",
    );
    assert!(out.contains("produces 2 nodes"), "{out}");
}

/// An optional child is between none and one, and neither is a node to stand
/// in for reliably.
#[test]
fn an_optional_child_is_not_guaranteed() {
    let out = transparent_err(
        "rule program = body:maybe -> prog;\n\
         rule maybe = stmt?;\n\
         rule stmt = v:ID -> s;\n",
    );
    assert!(out.contains("produces between 0 and 1 nodes"), "{out}");
}

/// Literals match text without producing a pair, so an alternative of nothing
/// but literals has no node at all.
#[test]
fn literals_alone_produce_no_node() {
    let out = transparent_err(
        "rule program = body:word -> prog;\n\
         rule word = \"yes\" | \"no\";\n",
    );
    assert!(out.contains("produces no node"), "{out}");
}

/// A token has a pair but no builder, so delegating to one has nowhere to go.
#[test]
fn a_lone_token_has_no_handler_to_delegate_to() {
    let out = transparent_err("rule program = ID;\n");
    assert!(out.contains("produces only the token `ID`"), "{out}");
    assert!(out.contains("no node of its own"), "{out}");
}

/// One node, but not always the same rule's — so there is no single type the
/// wrapper could be.
#[test]
fn a_choice_of_different_rules_is_ambiguous() {
    let out = transparent_err(
        "rule program = body:either -> prog;\n\
         rule either = (stmt | decl);\n\
         rule stmt = v:ID -> s;\n\
         rule decl = \"let\" v:ID -> d;\n",
    );
    assert!(out.contains("may be any of"), "{out}");
    assert!(out.contains("stmt"), "{out}");
    assert!(out.contains("decl"), "{out}");
}

// ---- and the cases that must stay quiet ------------------------------------

/// The plain alias. One rule, one node.
#[test]
fn an_alias_to_one_rule_is_fine() {
    transparent_ok(
        "rule program = body:atom -> prog;\n\
         rule atom = primary;\n\
         rule primary = v:ID -> p;\n",
    );
}

/// Literals around a single rule are still a single node — this is the shape
/// every parenthesised-expression rule has, and rejecting it would have been
/// the false positive that sank two earlier attempts at this check.
#[test]
fn literals_around_one_rule_are_still_one_node() {
    transparent_ok(
        "rule program = body:group -> prog;\n\
         rule group = \"(\" inner:stmt \")\" -> pass;\n\
         rule stmt = v:ID -> s;\n",
    );
}

/// A lookahead consumes nothing, so it cannot contribute a node.
#[test]
fn a_lookahead_contributes_no_node() {
    transparent_ok(
        "rule program = body:guarded -> prog;\n\
         rule guarded = !\"x\" inner:stmt -> pass;\n\
         rule stmt = v:ID -> s;\n",
    );
}

/// Repeating something that produces no node still produces no node, so it
/// does not make an otherwise-single child look unbounded.
#[test]
fn repeated_literals_do_not_inflate_the_count() {
    transparent_ok(
        "rule program = body:padded -> prog;\n\
         rule padded = \",\"* inner:stmt \",\"* -> pass;\n\
         rule stmt = v:ID -> s;\n",
    );
}

/// Both branches yield the same rule, so the wrapper stands in for it either
/// way.
#[test]
fn a_choice_yielding_the_same_rule_is_fine() {
    transparent_ok(
        "rule program = body:either -> prog;\n\
         rule either = (\"a\" stmt | \"b\" stmt);\n\
         rule stmt = v:ID -> s;\n",
    );
}

/// A silent rule produces no pair of its own — its body's pairs appear in its
/// place — so it is seen *through* rather than counted as one node.
///
/// Treating it as opaque left the one hole this check was written to close:
/// the wrapper got no child, generation emitted `pub type Wrapped =
/// Unresolved;` with a `build_?` it never defined, and the grammar failed in
/// rustc exactly as before.
#[test]
fn a_silent_rule_is_seen_through() {
    transparent_ok(
        "rule program = body:wrapped -> prog;\n\
         rule wrapped = quiet;\n\
         silent rule quiet = stmt;\n\
         rule stmt = v:ID -> s;\n",
    );
}

/// And seeing through it means its contents are counted, so a silent rule that
/// yields several nodes is caught like anything else.
#[test]
fn a_silent_rule_that_yields_several_nodes_is_caught() {
    let out = transparent_err(
        "rule program = body:wrapped -> prog;\n\
         rule wrapped = quiet;\n\
         silent rule quiet = stmt stmt;\n\
         rule stmt = v:ID -> s;\n",
    );
    assert!(out.contains("produces 2 nodes"), "{out}");
}

/// A silent rule that reaches itself has no finite count, so the check stays
/// quiet rather than recursing forever.
#[test]
fn a_recursive_silent_rule_terminates() {
    transparent_ok(
        "rule program = body:wrapped -> prog;\n\
         rule wrapped = quiet;\n\
         silent rule quiet = \"(\" quiet \")\" | stmt;\n\
         rule stmt = v:ID -> s;\n",
    );
}

// ---------------------------------------------------------------------------
// A binding name used more than once
//
// Generation keeps one accessor per name, so the cardinality it gets has to
// cover *every* occurrence. Keeping the first was right for a choice and wrong
// for a list.
// ---------------------------------------------------------------------------

fn card(rules: &str, label: &str, name: &str) -> nh_lower::Cardinality {
    let l = build_str(&format!(
        "grammar C;\n\
         use operators::none;\n\
         skip WS = \" \" | \"\\t\" | \"\\n\";\n\
         token ALPHA = @ \"a\"..\"z\";\n\
         token ID = @ ALPHA+;\n\
         token NUM = @ \"0\"..\"9\";\n\
         {rules}"
    ));
    binding(&l, label, name).cardinality
}

/// The head-and-tail list: one binding outside a repetition and the same name
/// inside it.
///
/// This was the bug. The first occurrence is `One`, the second `Many`, and
/// keeping the first gave an accessor returning a single node — so every item
/// after the first was silently dropped. No error, no warning, one item where
/// the grammar asks for a list.
#[test]
fn a_name_bound_inside_and_outside_a_repetition_is_a_list() {
    assert_eq!(
        card("rule r = items:ID (\",\" items:ID)* -> list;\n", "list", "items"),
        nh_lower::Cardinality::Many
    );
}

/// Two occurrences in sequence are two nodes, repetition or not.
#[test]
fn a_name_bound_twice_in_sequence_is_a_list() {
    assert_eq!(
        card("rule r = a:ID \",\" a:ID -> pair;\n", "pair", "a"),
        nh_lower::Cardinality::Many
    );
}

/// The case the old dedup was written for still collapses: the branches of a
/// choice are alternatives, so exactly one of them binds `x`.
#[test]
fn a_name_bound_in_both_branches_of_a_choice_is_still_one() {
    assert_eq!(
        card("rule r = (\"a\" x:ID | \"b\" x:NUM) -> either;\n", "either", "x"),
        nh_lower::Cardinality::One
    );
}

/// And when only one branch binds it, it may not be there at all.
#[test]
fn a_name_bound_in_only_one_branch_is_optional() {
    assert_eq!(
        card("rule r = (\"a\" x:ID | \"b\") -> maybe;\n", "maybe", "x"),
        nh_lower::Cardinality::Optional
    );
}

/// A repetition inside the binding is still the binding's own.
#[test]
fn a_repetition_inside_the_binding_still_counts() {
    assert_eq!(
        card("rule r = items:ID* -> some;\n", "some", "items"),
        nh_lower::Cardinality::Many
    );
    assert_eq!(
        card("rule r = value:ID? -> maybe;\n", "maybe", "value"),
        nh_lower::Cardinality::Optional
    );
}

/// A name bound once, plainly, is unaffected.
#[test]
fn a_name_bound_once_is_one() {
    assert_eq!(
        card("rule r = value:ID -> only;\n", "only", "value"),
        nh_lower::Cardinality::One
    );
}

/// And the tags are really all there, so the `Vec` accessor has something to
/// collect.
///
/// The cardinality decides whether generation emits `tagged` or `tagged_all`;
/// this pins the other half — that the grammar tags every element, so choosing
/// `tagged_all` actually recovers them. Together they are the difference
/// between three arguments and one.
#[test]
fn every_element_of_a_repeated_binding_is_tagged() {
    let l = build_str(
        "grammar C;\n\
         use operators::none;\n\
         skip WS = \" \" | \"\\t\" | \"\\n\";\n\
         token ALPHA = @ \"a\"..\"z\";\n\
         token ID = @ ALPHA+;\n\
         rule r = items:ID (\",\" items:ID)* -> list;\n",
    );
    let v = vm(&l.pest);
    let mut pairs = v.parse("r", "a, b, c").expect("`a, b, c` should parse");
    let node = pairs.next().expect("one `r` pair");

    // **Direct** children, because that is what `tagged_all` scans. If the
    // `("," items:ID)*` group put its elements one level down, the accessor
    // would find only the first and the list would silently lose the rest —
    // which is the bug this whole change is about, one layer lower.
    let tagged = node
        .into_inner()
        .filter(|p| p.as_node_tag() == Some("items"))
        .count();
    assert_eq!(
        tagged, 3,
        "all three elements should be direct children carrying the tag"
    );
}
