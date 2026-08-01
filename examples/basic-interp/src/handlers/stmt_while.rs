//! Handler for `stmt_while` — `WHILE cond ... WEND`.
//!
//! From `| "WHILE" lazy cond:expr EOL* lazy body:line* "WEND" -> while`
//!
//! The difference from `FOR` is which parameters are deferred. `FOR` computes
//! its bounds **once**, so only its body is `lazy`. A `WHILE` has to re-test
//! its condition on every pass, so the condition is `lazy` too — and calling
//! `.eval(..)` on it again is what re-evaluates it.
//!
//! The expression itself was folded by precedence when the tree was built, so
//! re-testing costs an evaluation and not a re-parse.

use nh_runtime::{Ctx, Result, Shared};

use crate::generated::ast::{Expr, Line};
use crate::generated::dispatch::{Eval, Values};
use crate::{Interp, Value};

pub fn run(
    host: &mut Interp,
    cond: &Shared<Expr>,
    body: &[Shared<Line>],
    cx: &mut Ctx,
) -> Result<Value> {
'outer: while {
        let test = cond.eval(host, cx)?;
        host.truthy(&test)
    } {
        for line in body {
            match line.eval(host, cx) {
                Ok(_) => {}
                Err(e) if e.is_signal("EXIT WHILE") => break 'outer,
                Err(e) if e.is_signal("CONTINUE WHILE") => break,
                Err(e) => return Err(e),
            }
        }
    }
    Ok(Value::Nothing)
}
