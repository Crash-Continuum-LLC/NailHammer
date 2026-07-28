//! Handler for `stmt_call` — `CALL name`.
//!
//! From `| "CALL" target:IDENT -> call`
//!
//! Runs a body defined earlier, and possibly far away in the source. The lines
//! come out of the interpreter rather than out of this node, which is the whole
//! point: a subroutine is a piece of program held as a value.

use nh_runtime::{Ctx, Name, Result};

use crate::generated::dispatch::Eval;
use crate::{Interp, Value};

pub fn run(host: &mut Interp, target: &Name, cx: &mut Ctx) -> Result<Value> {
    let Some(body) = host.subs.get(target.key()).cloned() else {
        // `.text()`, not `.key()`: report the spelling that was typed.
        return cx.err(format!("undefined subroutine `{}`", target.text()));
    };

    host.enter_call()?;
    let result = (|| {
        for line in &body {
            match line.eval(host, cx) {
                Ok(_) => {}
                Err(e) if e.is_signal("EXIT SUB") => return Ok(Value::Nothing),

                // A subroutine is a **boundary**. Without this, `EXIT FOR`
                // inside a sub would unwind into whatever loop happened to
                // call it — the loop is dynamically enclosing but not
                // lexically, and a jump landing somewhere the source does not
                // show is exactly the kind of thing nobody can debug.
                Err(e) if e.is_signal("EXIT FOR") || e.is_signal("CONTINUE FOR") => {
                    return cx.err("`FOR` control flow cannot cross out of a `SUB`")
                }
                Err(e) if e.is_signal("EXIT WHILE") || e.is_signal("CONTINUE WHILE") => {
                    return cx.err("`WHILE` control flow cannot cross out of a `SUB`")
                }

                Err(e) => return Err(e),
            }
        }
        Ok(Value::Nothing)
    })();
    host.leave_call();

    result
}
