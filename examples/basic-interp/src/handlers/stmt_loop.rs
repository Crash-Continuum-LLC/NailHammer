//! Handler for `stmt_loop` — `FOR i = a TO b [STEP s] ... NEXT [i]`.
//!
//! From this alternative of `rule stmt`:
//!
//! ```text
//! "FOR" var:IDENT "=" from:expr "TO" to:expr step:step_clause? EOL*
//!   lazy body:line*
//! "NEXT" closing:IDENT?  -> loop
//! ```
//!
//! **This is what `lazy` is for.** Every other parameter arrives evaluated,
//! which is what makes handlers short. A loop body cannot: it has to run once
//! per iteration, and running it zero times is a legitimate outcome. So `body`
//! is marked `lazy` and arrives as unevaluated lines this handler runs — as
//! many times as the loop says, and no more.
//!
//! Since M7 those lines are **owned**: this handler could store them on the
//! interpreter and run them long after returning, which is what makes
//! subroutines and jumps expressible at all.

use std::rc::Rc;

use nh_runtime::{Ctx, Name, Result};

use crate::generated::ast::Line;
use crate::generated::dispatch::Eval;
use crate::{Interp, Value};

// Six bindings plus `host` and `cx` is over clippy's limit. The grammar
// chose that list, not this file.
#[allow(clippy::too_many_arguments)]
pub fn run(
    host: &mut Interp,
    var: &Name,
    from: Value,
    to: Value,
    step: Option<Value>,
    body: &[Rc<Line>],
    closing: Option<&Name>,
    cx: &mut Ctx,
) -> Result<Value> {
    // `NEXT I` must name the loop it closes, when it names one at all. Compared
    // on `.key()` because the token folds case; reported with `.text()` because
    // telling someone they wrote `i` when they typed `I` reads as a bug.
    if let Some(closing) = closing {
        if closing.key() != var.key() {
            return cx.err(format!(
                "`NEXT {}` closes a loop over `{}`",
                closing.text(),
                var.text()
            ));
        }
    }

    let start = number(&from, "FOR start", cx)?;
    let limit = number(&to, "TO limit", cx)?;
    let step = match &step {
        Some(v) => number(v, "STEP", cx)?,
        None => 1.0,
    };

    // Worth checking rather than hanging: `STEP 0` never reaches its limit.
    if step == 0.0 {
        return cx.err("`STEP 0` would loop forever");
    }

    let name = var.key().to_string();
    let mut i = start;

    'outer: while (step > 0.0 && i <= limit) || (step < 0.0 && i >= limit) {
        host.store(name.clone(), Value::Num(i));

        for line in body {
            match line.eval(host, cx) {
                Ok(_) => {}
                // Only *this* loop's signals are caught here. An `EXIT WHILE`
                // raised inside a `WHILE` nested in this body is not ours, so
                // it falls through and keeps unwinding to the loop it names.
                Err(e) if e.is_signal("EXIT FOR") => break 'outer,
                Err(e) if e.is_signal("CONTINUE FOR") => break,
                Err(e) => return Err(e),
            }
        }

        i += step;
    }

    // BASIC leaves the counter where the loop stopped, which is one step past
    // the limit for a loop that ran to completion and the current value for one
    // that was cut short by `EXIT FOR`.
    host.store(name, Value::Num(i));
    Ok(Value::Nothing)
}

fn number(value: &Value, what: &str, cx: &mut Ctx) -> Result<f64> {
    match value {
        Value::Num(n) => Ok(*n),
        other => cx.err(format!("{what} must be a number, got `{other}`")),
    }
}
