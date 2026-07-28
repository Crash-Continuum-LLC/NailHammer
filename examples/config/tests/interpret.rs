//! End-to-end tests for the worked interpreter.
//!
//! These run the pipeline a user would: parse with the generated pest grammar,
//! dispatch through generated views into hand-written handlers.

use config_example::{Interp, Value};
use nh_runtime::{Ctx, SourceMap};

fn eval(source: &str) -> Result<Value, String> {
    let mut sources = SourceMap::new();
    let file = sources.add("test.conf", source);
    let mut cx = Ctx::new(sources);
    let mut interp = Interp;

    config_example::generated::eval_source(&mut interp, &mut cx, file).map_err(|errors| {
        errors
            .iter()
            .map(|d| d.render(cx.sources()))
            .collect::<Vec<_>>()
            .join("\n")
    })
}

fn table(v: Value) -> Vec<(String, Value)> {
    match v {
        Value::Table(f) => f,
        other => panic!("expected a table, got {other}"),
    }
}

#[test]
fn scalars_round_trip() {
    let t = table(eval("s = \"hi\"; n = 42; f = 1.5; y = true; no = false; z = null;").unwrap());
    assert_eq!(t[0], ("s".into(), Value::Str("hi".into())));
    assert_eq!(t[1], ("n".into(), Value::Num(42.0)));
    assert_eq!(t[2], ("f".into(), Value::Num(1.5)));
    assert_eq!(t[3], ("y".into(), Value::Bool(true)));
    assert_eq!(t[4], ("no".into(), Value::Bool(false)));
    assert_eq!(t[5], ("z".into(), Value::Null));
}

/// The bug that motivated `(#tag = x)*`: tagging the repetition rather than
/// each iteration silently dropped the first element of every list.
#[test]
fn no_element_of_a_repetition_is_dropped() {
    let t = table(eval("xs = [ 1 2 3 4 5 ];").unwrap());
    let Value::List(items) = &t[0].1 else {
        panic!("expected a list");
    };
    assert_eq!(items.len(), 5, "the first element must not vanish: {items:?}");
    assert_eq!(items[0], Value::Num(1.0));

    assert_eq!(table(eval("a = 1; b = 2; c = 3;").unwrap()).len(), 3);
    let t = table(eval("outer = { a = 1; b = 2; };").unwrap());
    let Value::Table(inner) = &t[0].1 else {
        panic!("expected a table");
    };
    assert_eq!(inner.len(), 2);
}

#[test]
fn nesting_works_to_arbitrary_depth() {
    let v = eval("a = { b = { c = { d = [ [ 1 ] ]; }; }; };").unwrap();
    assert_eq!(v.to_string(), "{a: {b: {c: {d: [[1]]}}}}");
}

#[test]
fn comments_and_whitespace_are_skipped() {
    let t = table(eval("# leading\n\n  a = 1; # trailing\n").unwrap());
    assert_eq!(t.len(), 1);
}

/// `reserved from IDENT` guards the identifier token in both directions.
#[test]
fn reserved_words_are_not_identifiers() {
    assert!(eval("true = 1;").is_err(), "`true` cannot be a key");
    // But a key that merely *contains* one is fine: the boundary guard means
    // `truely` is an identifier, not `true` followed by `ly`.
    assert!(eval("truely = 1;").is_ok());
}

#[test]
fn the_shipped_sample_evaluates() {
    let sample = include_str!("../sample.conf");
    let t = table(eval(sample).unwrap());
    let keys: Vec<&str> = t.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(
        keys,
        vec!["name", "version", "stable", "notes", "tags", "limits"]
    );
}
