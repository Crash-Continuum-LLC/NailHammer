//! Handler for `program`.
//!
//! From `rule program = SOI EOL* lazy lines:line* EOI -> doc;`
//!
//! This is the driver, and it is why `lines` is `lazy`. Every other handler in
//! this interpreter takes its children already evaluated; this one takes the
//! whole program unevaluated so it can decide *which line runs next* — which is
//! what a `GOTO` needs and what a fold cannot express.
//!
//! Two capabilities make it possible, and neither existed before the AST was
//! owned:
//!
//! * **Inspection.** The jump table reads each line's number without running
//!   the line. `Line` is a typed struct with a `label` field, not an opaque
//!   handle.
//! * **Storage.** The lines outlive any single evaluation, so the driver can
//!   come back to one it has already passed.

use std::collections::HashMap;
use std::rc::Rc;

use nh_runtime::{Ctx, Result};

use crate::generated::ast::Line;
use crate::generated::dispatch::Eval;
use crate::{Interp, Value};

pub fn run(host: &mut Interp, lines: &[Rc<Line>], cx: &mut Ctx) -> Result<Value> {
    let labels = jump_table(lines, cx)?;
    let mut pc = 0;

    while pc < lines.len() {
        match lines[pc].eval(host, cx) {
            Ok(_) => pc += 1,

            // `GOTO` unwinds to here and says where to go next.
            Err(e) if e.is_signal("goto") => {
                let target = host
                    .jump
                    .take()
                    .expect("`GOTO` sets its target before signalling");
                match labels.get(&target) {
                    Some(&i) => pc = i,
                    None => return cx.err(format!("there is no line numbered {target}")),
                }
            }

            Err(e) => return Err(e),
        }
    }

    Ok(Value::Nothing)
}

/// Line number → index, built by reading the lines rather than running them.
fn jump_table(lines: &[Rc<Line>], cx: &mut Ctx) -> Result<HashMap<String, usize>> {
    let mut labels = HashMap::new();
    for (i, line) in lines.iter().enumerate() {
        if let Some(label) = &line.label {
            // Two lines with one number would make a jump ambiguous, and
            // silently taking the first is the kind of thing that is very hard
            // to notice from inside the program.
            if labels.insert(label.clone(), i).is_some() {
                return cx.err(format!("line number {label} is used more than once"));
            }
        }
    }
    Ok(labels)
}
