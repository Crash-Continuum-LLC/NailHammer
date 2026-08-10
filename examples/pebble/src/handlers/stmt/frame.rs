//! `begin frame ... end frame`
//!
//! The body is `lazy`, so this handler decides when it runs — and here that
//! means running it with output redirected, so everything shown inside can be
//! framed rather than printed as it goes.

use nh_runtime::{Ctx, Result, Shared};
use crate::generated::ast::Stmt;
use crate::generated::dispatch::Eval;
use crate::{Interp, Value};

pub fn run(host: &mut Interp, body: &[Shared<Stmt>], cx: &mut Ctx) -> Result<Value> {
    // Take the output collected so far, so the frame captures only its own.
    let before = std::mem::take(&mut host.output);

    let mut result = Ok(Value::Null);
    for stmt in body {
        result = stmt.eval(host, cx);
        if result.is_err() {
            break;
        }
    }

    let inside = std::mem::replace(&mut host.output, before);
    let width = inside.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    host.output.push(format!("+-{}-+", "-".repeat(width)));
    for line in &inside {
        host.output
            .push(format!("| {line:<width$} |", width = width));
    }
    host.output.push(format!("+-{}-+", "-".repeat(width)));

    result
}
