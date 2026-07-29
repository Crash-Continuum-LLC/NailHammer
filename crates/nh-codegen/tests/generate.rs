//! Codegen shape tests.
//!
//! Behavioural proof that the generated code *works* lives in
//! `examples/config`, which compiles it and runs an interpreter. These tests
//! cover the shapes that are easy to get wrong and awkward to observe there.

use nh_codegen::{generate, Options, Policy};
use nh_lower::lower;
use nh_syntax::{resolve, SourceMap};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

fn gen(source: &str) -> nh_codegen::Generated {
    // A unique path per call: tests run in parallel and several use the same
    // grammar text, so hashing the content alone lets two threads write and
    // read the same file at once.
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join("nh-codegen-tests");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!(
        "g{}-{}.nh",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, source).unwrap();

    let mut sm = SourceMap::new();
    let ast = resolve(&mut sm, &path).unwrap_or_else(|e| panic!("{}", e.render(&sm)));
    let table = nh_operators::resolve(&ast, &mut sm).unwrap_or_else(|e| panic!("{}", e.render(&sm)));
    let lowered = lower(&ast, &table).unwrap_or_else(|e| panic!("{}", e.render(&sm)));
    generate(&ast, &table, &lowered, &Options::default())
}

/// The lowered grammar, for tests that need the alternatives rather than the
/// files generated from them.
fn lowered_of(source: &str) -> nh_lower::Lowered {
    let dir = std::env::temp_dir().join("nh-codegen-tests");
    std::fs::create_dir_all(&dir).unwrap();
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    let path = dir.join(format!(
        "l{}-{}.nh",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, source).unwrap();

    let mut sm = SourceMap::new();
    let ast = resolve(&mut sm, &path).unwrap_or_else(|e| panic!("{}", e.render(&sm)));
    let table = nh_operators::resolve(&ast, &mut sm).unwrap_or_else(|e| panic!("{}", e.render(&sm)));
    lower(&ast, &table).unwrap_or_else(|e| panic!("{}", e.render(&sm)))
}

fn file<'a>(g: &'a nh_codegen::Generated, path: &str) -> &'a str {
    &g.files
        .iter()
        .find(|f| f.path == path)
        .unwrap_or_else(|| panic!("no generated file `{path}`"))
        .contents
}

const BASIC: &str = r#"
grammar T;
use operators::none;
skip WS = " ";
token ALPHA = @ "a".."z";
token DIGIT = @ "0".."9";
token IDENT = @ ALPHA (ALPHA | DIGIT)*;
rule atom = item;
rule item
  = name:IDENT              -> named place
  | first:IDENT rest:IDENT* -> many
  | maybe:IDENT?            -> opt
  ;
"#;

#[test]
fn accessor_shapes_follow_cardinality() {
    let g = gen(BASIC);
    let views = file(&g, "generated/views.rs");

    assert!(views.contains("pub fn name(&self) -> Node<'i, Rule>"), "{views}");
    assert!(views.contains("pub fn rest(&self) -> Vec<Node<'i, Rule>>"), "{views}");
    assert!(
        views.contains("pub fn maybe(&self) -> Option<Node<'i, Rule>>"),
        "{views}"
    );
}

/// `.key()` exists only where the token folds case, so calling it on a
/// case-sensitive grammar is a compile error rather than a silent no-op.
#[test]
fn key_is_generated_only_for_case_insensitive_tokens() {
    let sensitive = gen(BASIC);
    assert!(
        !file(&sensitive, "generated/views.rs").contains("Ident<'i, Rule>"),
        "a case-sensitive grammar must not offer `.key()`"
    );

    let folding = gen(&BASIC.replace(
        "token IDENT = @ ALPHA (ALPHA | DIGIT)*;",
        "keywords case-insensitive;\ntoken IDENT = @ case-insensitive ALPHA (ALPHA | DIGIT)*;",
    ));
    assert!(
        file(&folding, "generated/views.rs").contains("Ident<'i, Rule>"),
        "a folding grammar must offer `.key()`"
    );
}

/// Built-ins live on the `View` trait so an inherent accessor generated from a
/// binding named `text` or `span` shadows them instead of colliding.
#[test]
fn a_binding_may_shadow_a_built_in_view_method() {
    let g = gen(&BASIC.replace("name:IDENT", "text:IDENT"));
    let views = file(&g, "generated/views.rs");

    assert!(views.contains("impl<'i> View<'i, Rule> for"), "{views}");
    assert!(
        views.contains("pub fn text(&self) -> Node<'i, Rule>"),
        "the binding's accessor must be inherent:\n{views}"
    );
}

#[test]
fn handlers_are_required_and_one_file_each() {
    let g = gen(BASIC);
    let dispatch = file(&g, "generated/dispatch.rs");

    // Required, not defaulted: adding an alternative must break the build.
    assert!(dispatch.contains("fn item_named(&mut self, name: &str, cx: &mut Ctx) -> Result<Self::Out>;"), "{dispatch}");
    assert!(dispatch.contains("macro_rules! nh_handlers"), "{dispatch}");
    assert!(dispatch.contains("$crate::handlers::item_named::run"), "{dispatch}");

    for expected in ["handlers/item_named.rs", "handlers/item_many.rs", "handlers/item_opt.rs"] {
        assert!(g.files.iter().any(|f| f.path == expected), "missing {expected}");
    }
}

/// DESIGN.md §5.4: generated files are always overwritten, stubs never are.
#[test]
fn regeneration_policy_is_explicit() {
    let g = gen(BASIC);
    for f in &g.files {
        let expected = if f.path.starts_with("handlers/") && f.path != "handlers/mod.rs" {
            Policy::OnceOnly
        } else {
            Policy::Generated
        };
        assert_eq!(f.policy, expected, "wrong policy for {}", f.path);
    }

    // Every always-regenerated file warns against editing.
    for f in g.files.iter().filter(|f| f.policy == Policy::Generated) {
        assert!(f.contents.contains("DO NOT EDIT"), "{} lacks a header", f.path);
    }
}

#[test]
fn place_variants_come_from_place_marked_alternatives() {
    let g = gen(BASIC);
    let place = file(&g, "generated/place.rs");
    assert!(place.contains("ItemNamed {"), "{place}");
    assert!(!place.contains("ItemMany {"), "only `place` alternatives:\n{place}");
}

/// `expr` is emitted when the table has operators, or when the grammar asks for
/// it by name — otherwise not at all, because an unreachable operator driver is
/// dead code in a file the user owns (DESIGN.md §11, standing constraint 6).
#[test]
fn expr_is_emitted_only_when_something_needs_it() {
    // BASIC uses `operators::none` and never mentions `expr`.
    let bare = gen(BASIC);
    let dispatch = file(&bare, "generated/dispatch.rs");
    assert!(
        !dispatch.contains("fn eval_expr"),
        "no operators and no reference means no driver:\n{dispatch}"
    );
    assert!(
        !dispatch.contains("use nh_runtime::ops::"),
        "and no import for it either:\n{dispatch}"
    );

    // ...but binding `expr` under `operators::none` is a legitimate way to
    // write a grammar that gains operators later without rewriting every
    // binding, so the reference alone is enough to keep it.
    let referenced = gen(&BASIC.replace("rule atom = item;", "rule atom = item;\nrule wrap = e:expr -> w;"));
    assert!(
        file(&referenced, "generated/dispatch.rs").contains("fn eval_expr"),
        "a grammar that binds `expr` must get one"
    );

    let no_table = gen(
        "grammar N;\nskip WS = \" \";\ntoken ALPHA = @ \"a\"..\"z\";\n\
         rule item = name:ALPHA -> named;\n",
    );
    let dispatch = file(&no_table, "generated/dispatch.rs");
    assert!(
        !dispatch.contains("fn eval_expr"),
        "no table means no expr rule and no expr hook:\n{dispatch}"
    );
}

/// A rule with no handler of its own is transparent when it produced exactly
/// one child — that covers both `value` → `value_string` and a plain alias like
/// `rule atom = primary;`. More children means the author meant to handle it.
#[test]
fn rules_without_handlers_delegate_to_their_only_child() {
    let g = gen(BASIC);
    // The descent happens once, while the tree is built, rather than on every
    // evaluation — so this now lives in the builder.
    let ast = file(&g, "generated/ast.rs");
    assert!(ast.contains("(Some(only), None) => Ok(only)"), "{ast}");
    assert!(ast.contains("add a `-> label`"), "{ast}");
}

#[test]
fn the_shipped_config_example_is_up_to_date() {
    // The checked-in generated code must match what the current generator
    // produces, or the worked example silently drifts from the toolkit.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/config");
    let mut sm = SourceMap::new();
    let ast = resolve(&mut sm, &root.join("config.nh")).unwrap();
    let table = nh_operators::resolve(&ast, &mut sm).unwrap();
    let lowered = lower(&ast, &table).unwrap();
    let g = generate(&ast, &table, &lowered, &Options::default());

    for f in g.files.iter().filter(|f| f.policy == Policy::Generated) {
        let on_disk = std::fs::read_to_string(root.join("src").join(&f.path))
            .unwrap_or_else(|e| panic!("{}: {e}", f.path));
        if on_disk == f.contents {
            continue;
        }
        // Report the first differing line rather than two whole files: a
        // failure here means "regenerate", and a 20KB dump buries that.
        let diff = on_disk
            .lines()
            .zip(f.contents.lines())
            .enumerate()
            .find(|(_, (a, b))| a != b)
            .map(|(i, (a, b))| format!("line {}:\n  on disk:   {a}\n  generated: {b}", i + 1))
            .unwrap_or_else(|| {
                format!(
                    "line count differs: {} on disk, {} generated",
                    on_disk.lines().count(),
                    f.contents.lines().count()
                )
            });
        panic!(
            "examples/config/src/{} is stale — re-run:\n  \
             nh build examples/config/config.nh -o examples/config/src/config.pest \
             --rust examples/config/src\n\n{diff}",
            f.path
        );
    }
}

// ---------------------------------------------------------------------------
// Discoverability
//
// The complaint these answer: opening a generated handler, it is not clear what
// `view` represents or how to use it. Types alone do not say — `Node` is
// returned both for a token you read and a rule you evaluate.
// ---------------------------------------------------------------------------

const DOCS: &str = r#"
grammar D;
use operators::core;
skip WS = " ";
token DIGIT = @ "0".."9";
token ALPHA = @ "a".."z";
token NUMBER = @ DIGIT+;
token IDENT = @ ALPHA+;
rule program = SOI stmts:stmt* EOI -> doc;
rule stmt = "let" name:IDENT "=" value:expr ";" -> bind;
rule atom = primary;
rule primary = digits:NUMBER -> num;
"#;

/// Hovering a view should show the grammar it came from, not just its name.
#[test]
fn a_view_quotes_its_grammar_alternative() {
    let views = file(&gen(DOCS), "generated/views.rs").to_string();
    assert!(
        views.contains(r#"/// "let" name:IDENT "=" value:expr ";" -> bind"#),
        "the view should quote its own grammar:\n{views}"
    );
}

/// The return type is `Node` either way, so the doc has to say which.
#[test]
fn accessor_docs_distinguish_a_token_from_a_rule() {
    let views = file(&gen(DOCS), "generated/views.rs").to_string();
    assert!(views.contains("`name` — the `IDENT` token"), "{views}");
    assert!(views.contains("`value` — the `expr` rule"), "{views}");
    assert!(views.contains("the text of the `IDENT` token"), "{views}");
    assert!(
        views.contains("the value of the `expr` rule, already evaluated"),
        "{views}"
    );
}

/// A view is read by whoever is debugging generated dispatch, so each accessor
/// should point at the parameter it feeds rather than at handler code that no
/// longer exists.
#[test]
fn an_accessor_names_the_parameter_it_feeds() {
    let views = file(&gen(DOCS), "generated/views.rs").to_string();
    assert!(
        views.contains("/// `stmts: Vec<Self::Out>`:"),
        "an accessor should say which handler parameter it becomes:\n{views}"
    );
    // And it must not show handler code, because a handler has no view.
    assert!(
        !views.contains("dispatch(host, view."),
        "views are mechanism; they should not teach a handler idiom:\n{views}"
    );
}

/// The parameters **are** the handler's inputs: no view, no traversal, no
/// dispatch. A stub that made you fetch anything would be the thing this
/// design exists to remove.
#[test]
fn a_stub_receives_its_bindings_as_parameters() {
    let g = gen(DOCS);
    let stub = file(&g, "handlers/stmt.rs");

    assert!(
        stub.contains("pub fn run<H: Handlers>(host: &mut H, name: &str, value: H::Out, cx: &mut Ctx) -> Result<H::Out>"),
        "bindings must arrive as parameters, in grammar order:\n{stub}"
    );
    for absent in ["view", "into_pair", "dispatch(", "View<"] {
        assert!(!stub.contains(absent), "a stub should never mention `{absent}`:\n{stub}");
    }
    assert!(stub.contains("compile_error!"), "{stub}");
    // And the header quotes the grammar too.
    assert!(stub.contains(r#"//! "let" name:IDENT "=" value:expr ";" -> bind"#), "{stub}");
}

/// A parameter's doc says what it *is*, so the reader never has to work out
/// whether a name is text, a value, or something unevaluated.
#[test]
fn every_parameter_is_documented_by_kind() {
    let g = gen(DOCS);
    let stub = file(&g, "handlers/stmt.rs");
    assert!(stub.contains("/// * `name` — the text of the `IDENT` token"), "{stub}");
    assert!(
        stub.contains("/// * `value` — the value of the `expr` rule, already evaluated"),
        "{stub}"
    );
}

/// A `lazy` binding arrives as owned data the handler runs when it chooses —
/// or keeps, or never runs at all.
#[test]
fn a_lazy_binding_arrives_unevaluated() {
    let g = gen(&DOCS.replace(
        r#"rule stmt = "let" name:IDENT "=" value:expr ";" -> bind;"#,
        r#"rule stmt = "when" cond:expr lazy body:stmt -> iff;"#,
    ));
    let stub = file(&g, "handlers/stmt.rs");
    assert!(
        stub.contains("body: &Shared<Stmt>"),
        "a lazy binding must not be evaluated for the handler:\n{stub}"
    );
    assert!(stub.contains("use nh_runtime::Shared;"), "{stub}");
    assert!(stub.contains("use crate::generated::ast::Stmt;"), "{stub}");
    assert!(stub.contains("**unevaluated**"), "the doc must say so:\n{stub}");
}

/// ...and a handler with nothing deferred should not import `Shared`, leaving an
/// unused import in a file the user now owns.
#[test]
fn a_stub_imports_only_what_it_uses() {
    let g = gen(DOCS);
    let stub = file(&g, "handlers/primary.rs");
    assert!(stub.contains("use crate::generated::dispatch::Handlers;"), "{stub}");
    assert!(!stub.contains("use nh_runtime::Shared;"), "{stub}");
}

/// A `lazy` binding is owned data, so it needs no operator table at all. The
/// old `Deferred` came from the driver, which meant `lazy` plus
/// `operators::none` named a type that was never emitted.
#[test]
fn a_lazy_binding_works_without_an_operator_table() {
    let g = gen(&BASIC.replace(
        "  | maybe:IDENT?            -> opt",
        "  | first:IDENT lazy rest:item -> held",
    ));
    let dispatch = file(&g, "generated/dispatch.rs");
    let ast = file(&g, "generated/ast.rs");

    assert!(dispatch.contains("rest: &Shared<Item>"), "{dispatch}");
    assert!(ast.contains("pub rest: Shared<Item>,"), "{ast}");
    assert!(
        !dispatch.contains("Deferred<"),
        "the borrowed handle is gone:\n{dispatch}"
    );
}

/// A binding to a case-folding token becomes an owned `Name` in the AST and a
/// `&Name` parameter. Both spellings survive; no lifetime does.
#[test]
fn a_folding_token_binding_is_an_owned_name() {
    let g = gen(&BASIC.replace(
        "token IDENT = @ ALPHA (ALPHA | DIGIT)*;",
        "keywords case-insensitive;\ntoken IDENT = @ case-insensitive ALPHA (ALPHA | DIGIT)*;",
    ));
    let dispatch = file(&g, "generated/dispatch.rs");
    let ast = file(&g, "generated/ast.rs");

    assert!(ast.contains("pub name: Name,"), "owned in the tree:\n{ast}");
    assert!(dispatch.contains("name: &Name"), "borrowed in the signature:\n{dispatch}");
    assert!(
        dispatch.contains("let name = &node.name;"),
        "a borrow, not a rebuild:\n{dispatch}"
    );
    assert!(
        !dispatch.contains("Ident<"),
        "the parse-borrowing form is gone:\n{dispatch}"
    );
}

/// Generated code is read by the user's linter, so a rule whose shape trips a
/// lint must carry the `allow` — the reader cannot act on it without editing
/// their grammar (DESIGN.md §11, standing constraint 6).
#[test]
fn a_wide_rule_does_not_warn_in_the_users_linter() {
    let g = gen(&BASIC.replace(
        "  | maybe:IDENT?            -> opt",
        "  | a:IDENT b:IDENT c:IDENT d:IDENT e:IDENT f:IDENT -> wide",
    ));
    assert!(
        file(&g, "generated/dispatch.rs").contains("#![allow(clippy::too_many_arguments)]"),
        "the generated trait method has eight parameters"
    );
    assert!(
        file(&g, "handlers/item_wide.rs").contains("#[allow(clippy::too_many_arguments)]"),
        "and so does the stub, which the user then owns"
    );
    // A narrow handler must not carry a pointless attribute.
    assert!(
        !file(&g, "handlers/item_named.rs").contains("too_many_arguments"),
        "only where it is needed"
    );
}

// ---------------------------------------------------------------------------
// The owned AST (M7)
// ---------------------------------------------------------------------------

const AST: &str = r#"
grammar A;
use operators::core;
keywords case-insensitive;
skip WS = " ";
token DIGIT = @ "0".."9";
token ALPHA = @ "a".."z";
token NUMBER = @ DIGIT+;
token IDENT = @ case-insensitive ALPHA+;
rule program = SOI lines:line* EOI -> doc;
rule line = body:stmt -> line;
rule stmt
  = "let" name:IDENT "=" value:expr ";"      -> bind
  | "loop" lazy body:line* "end"             -> repeat
  | value:expr ";"                           -> eval
  ;
rule atom = primary;
rule primary
  = digits:NUMBER      -> num
  | "(" inner:expr ")" -> pass
  ;
"#;

/// The whole point: a `lazy` binding becomes owned, `'static` data the
/// interpreter can keep. A `Deferred` borrowed the parse tree and could only
/// live for one handler call, which is why `GOTO`, subroutines, and closures
/// were inexpressible (DESIGN.md §9).
#[test]
fn a_lazy_binding_becomes_storable_owned_data() {
    let g = gen(AST);
    let ast = file(&g, "generated/ast.rs");
    assert!(
        ast.contains("pub body: Vec<Shared<Line>>,"),
        "a lazy repetition must be owned and shareable:\n{ast}"
    );
    assert!(!ast.contains("Deferred"), "no borrowed handle survives:\n{ast}");
    assert!(!ast.contains("'i"), "and no lifetime:\n{ast}");
}

/// A rule with several labelled alternatives is an enum over one struct each —
/// the same split the handler files already use.
#[test]
fn a_choice_rule_is_an_enum_of_its_alternatives() {
    let g = gen(AST);
    let ast = file(&g, "generated/ast.rs");
    assert!(ast.contains("pub enum Stmt {"), "{ast}");
    for v in ["Bind(Shared<StmtBind>),", "Repeat(Shared<StmtRepeat>),", "Eval(Shared<StmtEval>),"] {
        assert!(ast.contains(v), "missing `{v}`:\n{ast}");
    }
    assert!(ast.contains("pub struct StmtBind {"), "{ast}");
}

/// `rule atom = primary;` carries no node. Emitting a wrapper would put an
/// empty layer in every type that mentions an atom.
#[test]
fn an_alias_rule_becomes_a_type_alias() {
    let g = gen(AST);
    let ast = file(&g, "generated/ast.rs");
    assert!(ast.contains("pub type Atom = Primary;"), "{ast}");
}

/// `"(" inner:expr ")"` yields an `expr`, and the enum has to say so — the
/// literals around it contribute no node.
#[test]
fn a_transparent_alternative_is_typed_by_what_it_yields() {
    let g = gen(AST);
    let ast = file(&g, "generated/ast.rs");
    assert!(ast.contains("Expr(Shared<Expr>),"), "{ast}");
}

/// Operators are folded **once**, while the AST is built, rather than on every
/// evaluation — which is what makes re-testing a loop condition cheap.
#[test]
fn expressions_are_folded_into_the_ast() {
    let g = gen(AST);
    let ast = file(&g, "generated/ast.rs");
    assert!(ast.contains("pub enum Expr {"), "{ast}");
    assert!(ast.contains("Infix { lhs: Shared<Expr>, op: OpKind, rhs: Shared<Expr>, span: Span },"), "{ast}");
    assert!(ast.contains("Atom(Shared<Primary>),"), "the atom rule resolves through its alias:\n{ast}");
}

/// A folding token keeps both spellings, because losing either is a bug the
/// type can prevent: `.key()` to look up, `.text()` to report.
#[test]
fn a_folding_token_becomes_an_owned_name() {
    let g = gen(AST);
    let ast = file(&g, "generated/ast.rs");
    assert!(ast.contains("pub name: Name,"), "{ast}");

    // ...and a grammar without one must not import the type.
    let b = gen(BASIC);
    let plain = file(&b, "generated/ast.rs");
    assert!(
        plain.contains("use nh_runtime::Span;"),
        "a grammar with no folding token must not import `Name`:\n{plain}"
    );
}

/// A recovering rule needs somewhere to put the spans it recovered from.
#[test]
fn a_recovering_rule_gets_an_error_variant() {
    let g = gen(&AST.replace("rule atom = primary;", "recover line sync \";\";\nrule atom = primary;"));
    let ast = file(&g, "generated/ast.rs");
    assert!(ast.contains("Error(Span),"), "{ast}");
}

// ---------------------------------------------------------------------------
// Handler drift
// ---------------------------------------------------------------------------
//
// Most grammar edits are caught by the compiler. Two are not, because
// parameters are positional and Rust cannot see a name across a call.

/// **The one that matters.** Swapping two same-typed bindings compiles, runs,
/// and does the wrong thing. Before this check it was completely silent.
#[test]
fn reordering_same_typed_bindings_is_an_error() {
    let l = lowered_of(&BASIC.replace(
        "  = name:IDENT              -> named place",
        "  = first:IDENT second:IDENT -> named place",
    ));
    let named = l
        .alternatives
        .iter()
        .find(|a| a.pest_rule == "item_named")
        .unwrap();
    let handler = "pub fn run<H: Handlers>(host: &mut H, second: &str, first: &str, cx: &mut Ctx) -> Result<H::Out> {}";

    let drift = nh_codegen::drift::check(named, handler).expect("must report");
    assert!(drift.is_error(), "a reorder is a defect, not a nitpick");
    let msg = drift.message("handlers/item_named.rs");
    assert!(msg.contains("different order"), "{msg}");
    assert!(msg.contains("grammar:  first, second"), "{msg}");
    assert!(msg.contains("handler:  second, first"), "{msg}");
}

/// A rename still works — the values are right, the names are stale — so it
/// warns rather than failing the build.
#[test]
fn renaming_a_binding_is_a_warning() {
    let l = lowered_of(&BASIC.replace(
        "  = name:IDENT              -> named place",
        "  = first:IDENT second:IDENT -> named place",
    ));
    let named = l
        .alternatives
        .iter()
        .find(|a| a.pest_rule == "item_named")
        .unwrap();
    let handler = "pub fn run<H: Handlers>(host: &mut H, first: &str, other: &str, cx: &mut Ctx) -> Result<H::Out> {}";

    let drift = nh_codegen::drift::check(named, handler).expect("must report");
    assert!(!drift.is_error(), "a rename still computes the right answer");
    assert!(drift.message("h.rs").contains("names its parameters differently"));
}

/// The generated stub must never trip its own check.
#[test]
fn a_fresh_stub_does_not_drift() {
    let g = gen(BASIC);
    let l = lowered_of(BASIC);
    let named = l.alternatives.iter().find(|a| a.pest_rule == "item_named").unwrap();
    let stub = file(&g, "handlers/item_named.rs");
    assert_eq!(nh_codegen::drift::check(named, stub), None);
}

/// A handler somebody rewrote into a different shape is theirs. Guessing at it
/// would produce a warning nobody can act on.
#[test]
fn an_unreadable_signature_is_left_alone() {
    let l = lowered_of(BASIC);
    let named = l.alternatives.iter().find(|a| a.pest_rule == "item_named").unwrap();
    assert_eq!(nh_codegen::drift::check(named, "// rewritten by hand\n"), None);
}

/// Adding a binding is an arity error, and the compiler names the parameter and
/// its type. A vaguer warning ahead of it would be noise.
#[test]
fn a_changed_parameter_count_is_left_to_the_compiler() {
    let l = lowered_of(&BASIC.replace(
        "  = name:IDENT              -> named place",
        "  = name:IDENT extra:IDENT  -> named place",
    ));
    let named = l.alternatives.iter().find(|a| a.pest_rule == "item_named").unwrap();
    let stale = "pub fn run<H: Handlers>(host: &mut H, name: &str, cx: &mut Ctx) -> Result<H::Out> {}";
    assert_eq!(nh_codegen::drift::check(named, stale), None);
}

// ---------------------------------------------------------------------------
// Two shapes: interpreter and compiler
// ---------------------------------------------------------------------------

/// `truthy` is a question only a host with *values* can answer. A bytecode
/// emitter's `Out` stands for something the target machine computes later, so
/// there is nothing to inspect — and requiring it of every host forced a
/// compiler to write a `truthy` it could never answer and must never be asked.
#[test]
fn semantics_does_not_demand_what_a_compiler_cannot_answer() {
    let g = gen(DOCS);
    let dispatch = file(&g, "generated/dispatch.rs");

    // `Semantics` is the minimum every host can meet.
    let semantics = &dispatch[dispatch.find("pub trait Semantics").unwrap()..];
    let semantics = &semantics[..semantics.find("\n}").unwrap()];
    assert!(semantics.contains("type Out"), "{semantics}");
    assert!(
        !semantics.contains("fn truthy"),
        "a compiler cannot answer this:\n{semantics}"
    );

    assert!(dispatch.contains("pub trait Values: Semantics"), "{dispatch}");
    assert!(dispatch.contains("fn truthy(&self, value: &Self::Out) -> bool;"), "{dispatch}");
}

/// **The interpreter writes nothing.**
///
/// `if truthy(lhs) { rhs } else { lhs }` is not a decision anybody makes — it
/// is what `&&` *means* for a host with values. The only host-specific part is
/// `truthy`, which is already on `Values`. So `nh_handlers!` writes the rest,
/// and a user who never thinks about short-circuiting gets it right.
#[test]
fn short_circuiting_is_written_for_you() {
    let g = gen(DOCS);
    let dispatch = file(&g, "generated/dispatch.rs");

    let macro_at = dispatch
        .find("macro_rules! nh_handlers")
        .expect("the handler macro");
    let default_arm = &dispatch[macro_at..dispatch[macro_at..]
        .find("($host:ty, without short_circuit)")
        .map(|i| macro_at + i)
        .expect("an opt-out arm")];

    assert!(
        default_arm.contains("impl $crate::generated::dispatch::ShortCircuit for $host"),
        "`nh_handlers!(Interp)` must write this so nobody else has to:\n{default_arm}"
    );
    assert!(
        default_arm.contains("if self.truthy(&lhs)") && default_arm.contains("if !self.truthy(&lhs)"),
        "`||` stops on a truthy left and `&&` on a falsy one; getting that \
         backwards is silent:\n{default_arm}"
    );
}

/// A host that is not value-shaped opts out, and then must supply its own.
///
/// This is the *only* place the two shapes diverge, and it is one phrase long.
#[test]
fn a_host_without_values_can_opt_out_and_write_its_own() {
    let g = gen(DOCS);
    let dispatch = file(&g, "generated/dispatch.rs");

    let at = dispatch
        .find("($host:ty, without short_circuit) => {")
        .unwrap_or_else(|| panic!("no opt-out arm:\n{dispatch}"));
    let arm = &dispatch[at..];

    assert!(
        arm.contains("impl $crate::generated::dispatch::Handlers for $host"),
        "the opt-out arm still writes the handlers:\n{arm}"
    );
    assert!(
        !arm.contains("ShortCircuit for $host"),
        "opting out must actually leave it out, or the compiler's own impl \
         collides with a generated one:\n{arm}"
    );
    assert!(
        !arm.contains("truthy"),
        "the whole point of opting out is having no `truthy`:\n{arm}"
    );
}

/// The lazy roles live on their own trait, and are declared rather than
/// defaulted.
///
/// Two separate properties, and both matter:
///
/// * **Own trait**, so `nh_handlers!` can write the impl in its own block
///   without touching the `Operators` impl the user hand-writes.
/// * **Declared**, so a host that opted out and then forgot hears it from
///   rustc. There cannot be a correct default anyway: the body needs
///   `Values::truthy`, and a Rust default cannot require a bound its trait
///   lacks.
#[test]
fn a_lazy_role_is_declared_on_its_own_trait() {
    let g = gen(DOCS);
    let dispatch = file(&g, "generated/dispatch.rs");

    let sc_at = dispatch
        .find("pub trait ShortCircuit: Semantics {")
        .unwrap_or_else(|| panic!("no `ShortCircuit`:\n{dispatch}"));
    let sc = &dispatch[sc_at..dispatch[sc_at..].find("\n}").unwrap() + sc_at];

    for role in ["and_then", "or_else"] {
        let at = sc
            .find(&format!("fn {role}("))
            .unwrap_or_else(|| panic!("`{role}` is not on `ShortCircuit`:\n{sc}"));
        let decl = &sc[at..];
        let end = decl.find(';').unwrap_or(usize::MAX);
        let body = decl.find('{').unwrap_or(usize::MAX);
        assert!(
            end < body,
            "`{role}` must be declared, not defaulted:\n{decl}"
        );
    }

    // The driver reaches them through the bound it already has.
    assert!(
        dispatch.contains("pub trait Operators: ShortCircuit {"),
        "{dispatch}"
    );

    // And the strict roles keep their defaults: not using `%` is not an error.
    let ops_at = dispatch.find("pub trait Operators: ShortCircuit {").unwrap();
    let ops = &dispatch[ops_at..];
    assert!(
        ops.contains("Error::unsupported"),
        "strict roles still default:\n{ops}"
    );
    assert!(
        !ops[..ops.find("pub trait Handlers").unwrap()].contains("fn and_then"),
        "a lazy role must not also be on `Operators`:\n{ops}"
    );
}

/// A grammar with nothing lazy gets no trait, no impl, and no opt-out arm.
#[test]
fn a_table_without_lazy_operators_gets_none_of_this() {
    let g = gen(BASIC);
    let dispatch = file(&g, "generated/dispatch.rs");

    assert!(
        !dispatch.contains("pub trait ShortCircuit"),
        "nothing short-circuits here:\n{dispatch}"
    );
    assert!(
        dispatch.contains("pub trait Operators: Semantics {"),
        "no lazy roles means no supertrait:\n{dispatch}"
    );
    // Offering `without short_circuit` where there is nothing to opt out of
    // would be a phrase that parses and means nothing.
    assert!(
        !dispatch.contains("without short_circuit"),
        "{dispatch}"
    );
}
