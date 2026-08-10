//! Handler for `primary_call_fn` — `f(1, 2)` inside an expression.
//!
//! From `= name:IDENT "(" args:arg_list? ")" -> call_fn`
//!
//! Unlike `CALL`, this is an **expression**: it produces a value, so it can
//! appear anywhere an operand can, and the operator driver folds it as an atom.

use nh_runtime::{Ctx, Name, Result};

use crate::generated::dispatch::Eval;
use crate::{Interp, Value};

pub fn run(host: &mut Interp, name: &Name, args: Option<Value>, cx: &mut Ctx) -> Result<Value> {
    let Some(f) = host.funcs.get(name.key()).cloned() else {
        return cx.err(format!("undefined function `{}`", name.text()));
    };

    let args = match args {
        Some(Value::List(items)) => items,
        Some(one) => vec![one],
        None => Vec::new(),
    };

    if args.len() != f.params.len() {
        return cx.err(format!(
            "`{}` takes {} argument(s), got {}",
            name.text(),
            f.params.len(),
            args.len()
        ));
    }

    // Parameters go in a frame of their own, so a recursive call cannot write
    // over its caller's copy.
    let frame = f.params.iter().cloned().zip(args).collect();

    host.enter_call()?;
    host.frames.push(frame);

    let mut result = None;
    let mut failure = None;
    for line in &f.body {
        match line.eval(host, cx) {
            Ok(_) => {}
            Err(e) if e.is_signal("RETURN") => {
                result = host.ret.take();
                break;
            }
            // A function is a boundary, exactly as a `SUB` is: loop control
            // must not unwind into whichever loop happened to call it.
            Err(e) if e.is_signal("EXIT FOR") || e.is_signal("CONTINUE FOR") => {
                failure = Some(cx.error("`FOR` control flow cannot cross out of a `FUNCTION`"));
                break;
            }
            Err(e) if e.is_signal("EXIT WHILE") || e.is_signal("CONTINUE WHILE") => {
                failure = Some(cx.error("`WHILE` control flow cannot cross out of a `FUNCTION`"));
                break;
            }
            Err(e) => {
                failure = Some(e);
                break;
            }
        }
    }

    host.frames.pop();
    host.leave_call();

    if let Some(e) = failure {
        return Err(e);
    }
    match result {
        Some(v) => Ok(v),
        None => cx.err(format!("`{}` ended without a `RETURN`", name.text())),
    }
}
