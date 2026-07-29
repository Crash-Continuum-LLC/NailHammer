//! What `nh init` offers to put in the box.
//!
//! A scaffold that only ever emits `let`/`print` makes you write the
//! interesting parts — loops, functions — from nothing, against a grammar
//! language you have not learned yet. These are the parts where a starting
//! point is worth most, so they are offered rather than left out.
//!
//! # Two axes
//!
//! **Style** is syntax: `while (x) { }` against `WHILE x ... WEND`. It changes
//! the grammar and nothing else.
//!
//! **Features** are capability: loops, functions. They add alternatives to
//! `rule stmt`, and a handler apiece.
//!
//! # The property worth preserving
//!
//! Both styles produce the **same handler signatures**, because both bind the
//! same names to the same shapes. `WHILE cond ... WEND` and `while cond { }`
//! each give `run(host, cond: &Rc<Expr>, body: &Rc<Block>, cx)`, so
//! `handlers/stmt_while.rs` is one file used by both.
//!
//! That is not a coincidence to be grateful for — it is the point of binding by
//! name instead of position, and `both_styles_share_their_handlers` in
//! `tests/init.rs` fails if it stops being true.

use std::fmt;

/// The syntactic flavour of the scaffolded language.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Style {
    /// Braces and semicolons. Statements end at `;`, blocks are `{ }`.
    #[default]
    C,
    /// Line-oriented, like BASIC. A newline ends a statement, and blocks are
    /// closed by a keyword — `WEND`, `NEXT`, `END FUNCTION`.
    ///
    /// This is a genuinely different grammar rather than C wearing a hat: the
    /// skip set excludes `\n`, so newlines are significant, and assignment is a
    /// statement rather than an operator because `=` already means equality.
    Basic,
}

impl Style {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.to_ascii_lowercase().as_str() {
            "c" => Ok(Style::C),
            "basic" => Ok(Style::Basic),
            other => Err(format!(
                "unknown style `{other}`; expected `c` or `basic`"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Style::C => "c",
            Style::Basic => "basic",
        }
    }
}

impl fmt::Display for Style {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// An optional capability.
///
/// Deliberately coarse. `while` without `break` is a toy, and `fn` without
/// `return` is not a function, so each of these is a whole working idea rather
/// than a keyword.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Feature {
    /// `while`, `for`, `do`, and the two ways out of them.
    ///
    /// `break`/`continue` come along because a loop you cannot leave early is a
    /// toy — and because they are the sharpest illustration of the two shapes:
    /// an interpreter unwinds with `Error::Signal`, a compiler keeps a patch
    /// list and back-fills jump targets.
    Loops,
    /// Definitions, calls, parameters, locals, `return`, and recursion.
    Functions,
}

impl Feature {
    pub const ALL: &'static [Feature] = &[Feature::Loops, Feature::Functions];

    pub fn parse(s: &str) -> Result<Self, String> {
        match s.to_ascii_lowercase().as_str() {
            "loops" | "loop" => Ok(Feature::Loops),
            "functions" | "function" | "fn" => Ok(Feature::Functions),
            other => Err(format!(
                "unknown feature `{other}`; expected `loops`, `functions`, `all`, or `none`"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Feature::Loops => "loops",
            Feature::Functions => "functions",
        }
    }

    /// What the picker shows.
    pub fn summary(self) -> &'static str {
        match self {
            Feature::Loops => "while, for, do — with break and continue",
            Feature::Functions => "definitions, calls, parameters, return, recursion",
        }
    }
}

impl fmt::Display for Feature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The selected set, kept sorted and unique so ordering never leaks into
/// generated output.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Features(Vec<Feature>);

impl Features {
    pub fn none() -> Self {
        Features(Vec::new())
    }

    pub fn all() -> Self {
        Features(Feature::ALL.to_vec())
    }

    pub fn from_list(list: &[Feature]) -> Self {
        let mut v = list.to_vec();
        v.sort();
        v.dedup();
        Features(v)
    }

    /// Parses `--with loops,functions`, or `all`, or `none`.
    pub fn parse(spec: &str) -> Result<Self, String> {
        let spec = spec.trim();
        if spec.is_empty() || spec.eq_ignore_ascii_case("none") {
            return Ok(Features::none());
        }
        if spec.eq_ignore_ascii_case("all") {
            return Ok(Features::all());
        }

        let mut out = Vec::new();
        for part in spec.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            out.push(Feature::parse(part)?);
        }
        Ok(Features::from_list(&out))
    }

    pub fn has(&self, f: Feature) -> bool {
        self.0.contains(&f)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = Feature> + '_ {
        self.0.iter().copied()
    }
}

impl fmt::Display for Features {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_empty() {
            return f.write_str("none");
        }
        let names: Vec<&str> = self.0.iter().map(|x| x.as_str()).collect();
        f.write_str(&names.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spec_parses_in_any_order_and_normalises() {
        assert_eq!(
            Features::parse("functions,loops").unwrap(),
            Features::parse("loops, functions").unwrap(),
            "the order somebody types them must not reach the output"
        );
        assert_eq!(Features::parse("all").unwrap(), Features::all());
        assert_eq!(Features::parse("none").unwrap(), Features::none());
        assert_eq!(Features::parse("").unwrap(), Features::none());
        assert_eq!(
            Features::parse("loops,loops").unwrap(),
            Features::from_list(&[Feature::Loops])
        );
    }

    /// A typo must not silently scaffold less than was asked for.
    #[test]
    fn an_unknown_feature_is_refused_by_name() {
        let e = Features::parse("loops,closures").unwrap_err();
        assert!(e.contains("closures"), "{e}");
        assert!(e.contains("`loops`"), "it should say what is on offer: {e}");
    }

    #[test]
    fn styles_round_trip() {
        for s in [Style::C, Style::Basic] {
            assert_eq!(Style::parse(s.as_str()).unwrap(), s);
        }
        assert!(Style::parse("pascal").unwrap_err().contains("pascal"));
    }
}

// ---------------------------------------------------------------------------
// Grammar fragments
//
// Each feature contributes reserved words, `rule stmt` alternatives, extra
// rules, and (for functions) one `primary` alternative. Per style, because
// that is what a style *is* — and only that, because the handlers are shared.
// ---------------------------------------------------------------------------

/// What one feature adds to a grammar.
#[derive(Default)]
pub struct Fragment {
    pub reserved: &'static str,
    pub stmt: &'static str,
    pub rules: &'static str,
    pub primary: &'static str,
}

/// The four slots, filled in for a whole selection.
pub struct GrammarParts {
    pub reserved: String,
    pub stmt_loops: String,
    pub stmt_functions: String,
    pub rules: String,
    pub primary: String,
}

impl Features {
    pub fn grammar_parts(&self, style: Style) -> GrammarParts {
        let mut parts = GrammarParts {
            reserved: String::new(),
            stmt_loops: String::new(),
            stmt_functions: String::new(),
            rules: String::new(),
            primary: String::new(),
        };
        for f in self.iter() {
            let frag = fragment(f, style);
            parts.reserved.push_str(frag.reserved);
            match f {
                Feature::Loops => parts.stmt_loops.push_str(frag.stmt),
                Feature::Functions => parts.stmt_functions.push_str(frag.stmt),
            }
            parts.rules.push_str(frag.rules);
            parts.primary.push_str(frag.primary);
        }
        parts
    }
}

fn fragment(f: Feature, style: Style) -> Fragment {
    match (f, style) {
        (Feature::Loops, Style::C) => Fragment {
            reserved: r#" "while" "for" "to" "do" "break" "continue""#,
            // `lazy cond` matters as much as `lazy body`: a loop condition is
            // re-tested every iteration, so evaluating it once would be wrong.
            stmt: concat!(
                "  | \"while\" lazy cond:expr lazy body:block                   -> while\n",
                "  | \"for\" var:IDENT \"=\" from:expr \"to\" to:expr lazy body:block -> for\n",
                "  | \"do\" lazy body:block \"while\" lazy cond:expr \";\"          -> do\n",
                "  | \"break\" \";\"                                              -> break\n",
                "  | \"continue\" \";\"                                           -> continue\n",
            ),
            ..Fragment::default()
        },
        (Feature::Loops, Style::Basic) => Fragment {
            reserved: r#" "WHILE" "WEND" "FOR" "TO" "NEXT" "DO" "LOOP" "EXIT" "CONTINUE""#,
            stmt: concat!(
                "  | \"WHILE\" lazy cond:expr EOL*\n",
                "      lazy body:block\n",
                "    \"WEND\"                                                 -> while\n",
                "  | \"FOR\" var:IDENT \"=\" from:expr \"TO\" to:expr EOL*\n",
                "      lazy body:block\n",
                "    \"NEXT\"                                                 -> for\n",
                "  | \"DO\" EOL*\n",
                "      lazy body:block\n",
                "    \"LOOP\" \"WHILE\" lazy cond:expr                          -> do\n",
                "  | \"EXIT\"                                                 -> break\n",
                "  | \"CONTINUE\"                                             -> continue\n",
            ),
            ..Fragment::default()
        },

        (Feature::Functions, Style::C) => Fragment {
            reserved: r#" "fn" "return""#,
            stmt: concat!(
                "  | \"fn\" name:IDENT \"(\" lazy params:param_list? \")\" lazy body:block -> fn\n",
                "  | \"return\" value:expr? \";\"                                 -> return\n",
            ),
            // `params` is `lazy` because a definition wants the parameter
            // *names*. Without it they would arrive evaluated — that is, looked
            // up as variables that do not exist yet.
            rules: concat!(
                "\nrule param_list = first:IDENT rest:more_param* -> list;\n",
                "rule more_param = \",\" name:IDENT -> one;\n",
            ),
            primary: "  | name:IDENT \"(\" first:expr? rest:more_arg* \")\" -> call\n",
        },
        (Feature::Functions, Style::Basic) => Fragment {
            reserved: r#" "FUNCTION" "RETURN""#,
            stmt: concat!(
                "  | \"FUNCTION\" name:IDENT \"(\" lazy params:param_list? \")\" EOL*\n",
                "      lazy body:block\n",
                "    \"END\" \"FUNCTION\"                                       -> fn\n",
                "  | \"RETURN\" value:expr?                                   -> return\n",
            ),
            rules: concat!(
                "\nrule param_list = first:IDENT rest:more_param* -> list;\n",
                "rule more_param = \",\" name:IDENT -> one;\n",
            ),
            primary: "  | name:IDENT \"(\" first:expr? rest:more_arg* \")\" -> call\n",
        },
    }
}

/// `more_arg` is shared by every caller of `primary_call`, so it is emitted
/// once rather than per feature.
pub const ARG_RULE: &str = "rule more_arg = \",\" value:expr -> one;\n";

// ---------------------------------------------------------------------------
// The picker
// ---------------------------------------------------------------------------

/// Asks, when there is somebody there to ask.
///
/// `nh init` is usually run by a person at a prompt, and a person should not
/// have to know the flag names to be offered the choice. But it is also run by
/// scripts, CI, and this crate's own tests, so:
///
/// * an explicit `--style` or `--with` always wins and never prompts;
/// * with neither, a terminal gets the questions and anything else gets the
///   defaults. A build that hangs waiting for input nobody can give is worse
///   than a build that picks something reasonable.
pub fn choose(
    style_flag: Option<&str>,
    with_flag: Option<&str>,
    interactive: bool,
) -> Result<(Style, Features), String> {
    let style = style_flag.map(Style::parse).transpose()?;
    let features = with_flag.map(Features::parse).transpose()?;

    if let (Some(s), Some(f)) = (style, features.clone()) {
        return Ok((s, f));
    }
    if !interactive {
        return Ok((style.unwrap_or_default(), features.unwrap_or_default()));
    }

    let style = match style {
        Some(s) => s,
        None => ask_style()?,
    };
    let features = match features {
        Some(f) => f,
        None => ask_features()?,
    };
    Ok((style, features))
}

fn prompt(question: &str) -> Result<String, String> {
    use std::io::Write as _;
    print!("{question}");
    std::io::stdout()
        .flush()
        .map_err(|e| format!("cannot write to the terminal: {e}"))?;

    let mut line = String::new();
    match std::io::stdin().read_line(&mut line) {
        // EOF: the terminal went away mid-question. Taking the default is
        // better than looping on an empty read forever.
        Ok(0) => Ok(String::new()),
        Ok(_) => Ok(line.trim().to_string()),
        Err(e) => Err(format!("cannot read from the terminal: {e}")),
    }
}

fn ask_style() -> Result<Style, String> {
    println!("\nWhich syntax?");
    println!("  1) c      braces and semicolons   —  while x {{ print x; }}");
    println!("  2) basic  line-oriented           —  WHILE x ... WEND");

    loop {
        let answer = prompt("style [1]: ")?;
        match answer.as_str() {
            "" | "1" => return Ok(Style::C),
            "2" => return Ok(Style::Basic),
            other => match Style::parse(other) {
                Ok(s) => return Ok(s),
                Err(e) => println!("  {e}"),
            },
        }
    }
}

fn ask_features() -> Result<Features, String> {
    println!("\nWhat should it come with?");
    for (i, f) in Feature::ALL.iter().enumerate() {
        println!("  {}) {:<10} {}", i + 1, f.as_str(), f.summary());
    }
    println!("  Enter numbers separated by commas, `all`, or blank for none.");

    loop {
        let answer = prompt("features [all]: ")?;
        if answer.is_empty() {
            return Ok(Features::all());
        }
        if answer.eq_ignore_ascii_case("none") {
            return Ok(Features::none());
        }

        // Numbers and names in the same list, so `1,functions` works.
        let mut picked = Vec::new();
        let mut bad = None;
        for part in answer.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            match part.parse::<usize>() {
                Ok(n) if (1..=Feature::ALL.len()).contains(&n) => picked.push(Feature::ALL[n - 1]),
                Ok(n) => bad = Some(format!("there is no option {n}")),
                Err(_) => match Feature::parse(part) {
                    Ok(f) => picked.push(f),
                    Err(e) => bad = Some(e),
                },
            }
        }
        match bad {
            Some(e) => println!("  {e}"),
            None => return Ok(Features::from_list(&picked)),
        }
    }
}

// ---------------------------------------------------------------------------
// Handler files and host chunks
// ---------------------------------------------------------------------------

/// Which handler modules a selection needs.
///
/// One list, not two: the same file names serve both syntax styles, because
/// both bind the same names to the same shapes. Only `line` is style-specific,
/// and only because a `;` needs no wrapper.
pub fn handler_names(style: Style, features: &Features) -> Vec<&'static str> {
    let mut v = vec![
        "program",
        "block",
        "stmt_bind",
        "stmt_print",
        "stmt_iff",
        "else_tail",
        "stmt_eval",
        "primary_num",
        "primary_var",
    ];
    if style == Style::Basic {
        v.push("line");
    }
    if features.has(Feature::Loops) {
        v.extend(["stmt_while", "stmt_for", "stmt_do", "stmt_break", "stmt_continue"]);
    }
    if features.has(Feature::Functions) {
        v.extend([
            "stmt_fn",
            "stmt_return",
            "primary_call",
            "param_list",
            "more_param",
            "more_arg",
        ]);
    }
    v.sort_unstable();
    v
}

/// The four holes a feature can fill in `src/lib.rs`.
#[derive(Default)]
pub struct HostChunks {
    /// Types beside `Interp`.
    pub types: String,
    /// Extra fields on `Interp`.
    pub state: String,
    /// Extra methods in `impl Interp`.
    pub methods: String,
    /// Extra `Op` variants (compiler only).
    pub vm_ops: String,
    /// Extra arms in the VM's `match` (compiler only).
    pub vm_exec: String,
}

impl Features {
    pub fn host_chunks(&self, is_compiler: bool) -> HostChunks {
        let mut c = HostChunks::default();
        for f in self.iter() {
            let part = host_chunk(f, is_compiler);
            c.types.push_str(part.types.as_str());
            c.state.push_str(part.state.as_str());
            c.methods.push_str(part.methods.as_str());
            c.vm_ops.push_str(part.vm_ops.as_str());
            c.vm_exec.push_str(part.vm_exec.as_str());
        }
        c
    }
}

fn host_chunk(f: Feature, is_compiler: bool) -> HostChunks {
    match (f, is_compiler) {
        // An interpreter needs nothing for loops: `break` and `continue` are
        // signals, and `Error::Signal` is already in the runtime.
        (Feature::Loops, false) => HostChunks::default(),

        (Feature::Loops, true) => HostChunks {
            types: LOOP_FRAME.into(),
            state: "    /// One per loop being compiled. Innermost last.\n    pub loops: Vec<LoopFrame>,\n".into(),
            methods: LOOP_METHODS.into(),
            vm_ops: "    /// `<=`, for a counting loop's test.\n    Le,\n".into(),
            vm_exec: "                Op::Le => bin(&mut stack, |a, b| if a <= b { 1.0 } else { 0.0 }),\n".into(),
        },

        (Feature::Functions, false) => HostChunks {
            types: INTERP_FUNCTION.into(),
            state: INTERP_FN_STATE.into(),
            methods: INTERP_CALL.into(),
            ..HostChunks::default()
        },

        (Feature::Functions, true) => HostChunks {
            types: COMPILER_FNINFO.into(),
            state: "    /// Where each function starts, and how many arguments it takes.\n    pub fns: HashMap<String, FnInfo>,\n".into(),
            methods: COMPILER_FN_METHODS.into(),
            vm_ops: COMPILER_FN_OPS.into(),
            vm_exec: COMPILER_FN_EXEC.into(),
        },
    }
}

// ---------------------------------------------------------------------------
// The chunks themselves.
//
// Rust source as text, which is what a template is. They are `const`s rather
// than files because each is a few lines that only makes sense beside the slot
// it fills.
// ---------------------------------------------------------------------------

const LOOP_FRAME: &str = r##"/// One loop being compiled.
///
/// A compiler cannot unwind — the loop has not run yet — so `break` and
/// `continue` emit a jump with no target and record its index here. The loop
/// handler fills them in once it knows where its own end and step are.
#[derive(Debug, Default)]
pub struct LoopFrame {
    /// Where the loop starts, so frames can be told apart.
    pub start: usize,
    pub breaks: Vec<usize>,
    pub continues: Vec<usize>,
}

"##;

const LOOP_METHODS: &str = r##"
    pub fn emit_add(&mut self) {
        self.emit(Op::Add)
    }

    pub fn emit_le(&mut self) {
        self.emit(Op::Le)
    }

    /// Starts collecting the jumps out of a loop.
    pub fn enter_loop(&mut self, start: usize) {
        self.loops.push(LoopFrame {
            start,
            ..LoopFrame::default()
        });
    }

    /// Finishes a loop: every `break` lands after it, every `continue` on
    /// `step`.
    ///
    /// `step` is not always the loop's start. In a counting loop it is the
    /// increment, because a `continue` that skipped the increment would never
    /// terminate.
    pub fn exit_loop(&mut self, step: usize) {
        let frame = self.loops.pop().expect("exit_loop without enter_loop");
        let end = self.here();
        for at in frame.breaks {
            self.patch_to(at, end);
        }
        for at in frame.continues {
            self.patch_to(at, step);
        }
    }

    /// Records a `break`. False when there is no loop to leave.
    pub fn break_to(&mut self, jump: usize) -> bool {
        match self.loops.last_mut() {
            Some(frame) => {
                frame.breaks.push(jump);
                true
            }
            None => false,
        }
    }

    pub fn continue_to(&mut self, jump: usize) -> bool {
        match self.loops.last_mut() {
            Some(frame) => {
                frame.continues.push(jump);
                true
            }
            None => false,
        }
    }
"##;

const INTERP_FUNCTION: &str = r##"/// A function, as stored by a definition and used by a call.
///
/// The body is an `Rc<Block>` — a node from the owned tree, which outlives the
/// parse. That is what makes storing it possible at all: there is no borrow of
/// the source text here to keep alive.
#[derive(Clone, Debug)]
pub struct Function {
    pub params: Vec<String>,
    pub body: std::rc::Rc<generated::ast::Block>,
}

"##;

const INTERP_FN_STATE: &str = r##"    pub fns: HashMap<String, Function>,
    /// Where `return` leaves its value. It does not ride on the signal: the
    /// runtime has no idea what your values are, and should not have to.
    pub returning: Option<Value>,
"##;

const INTERP_CALL: &str = r##"
    /// Calls a function: bind the arguments, run the body, catch the `return`.
    ///
    /// Recursion works because each call pushes its own frame, and `get`/`set`
    /// look at the innermost one. Nothing here is reentrant by accident — the
    /// frame is popped on every path out, including the error path.
    pub fn call(
        &mut self,
        name: &str,
        args: Vec<Value>,
        cx: &mut nh_runtime::Ctx,
    ) -> nh_runtime::Result<Value> {
        let Some(f) = self.fns.get(name).cloned() else {
            return cx.err(format!("undefined function `{name}`"));
        };
        if args.len() != f.params.len() {
            return cx.err(format!(
                "`{name}` takes {} argument(s), got {}",
                f.params.len(),
                args.len()
            ));
        }

        let frame: HashMap<String, Value> = f.params.iter().cloned().zip(args).collect();
        self.locals.push(frame);

        let outcome = {
            use generated::dispatch::Eval;
            f.body.eval(self, cx)
        };

        self.locals.pop();

        match outcome {
            // Fell off the end without returning.
            Ok(_) => Ok(Value::Unit),
            // The signal means a `return` ran; the value is where it left it.
            Err(nh_runtime::Error::Signal { label: "return", .. }) => {
                Ok(self.returning.take().unwrap_or(Value::Unit))
            }
            // `break` from inside a function body is not a way out of a loop
            // outside it. Letting it through would be a jump across a call.
            Err(nh_runtime::Error::Signal { label, .. }) => {
                cx.err(format!("`{label}` is not inside anything that handles it"))
            }
            Err(other) => Err(other),
        }
    }
"##;

const COMPILER_FNINFO: &str = r##"/// Where a function starts, and how many arguments it takes.
#[derive(Clone, Copy, Debug)]
pub struct FnInfo {
    pub addr: usize,
    pub arity: usize,
}

/// One call in progress, at run time.
#[derive(Debug)]
struct Frame {
    ret: usize,
    locals: std::collections::HashMap<String, f64>,
}

"##;

const COMPILER_FN_METHODS: &str = r##"
    pub fn emit_call(&mut self, name: &str, argc: usize) {
        self.emit(Op::Call(name.to_string(), argc));
    }

    pub fn emit_return(&mut self) {
        self.emit(Op::Return);
    }
"##;

const COMPILER_FN_OPS: &str = r##"    /// Call by name, with a known argument count. Resolved when the program
    /// runs rather than patched here, so a function can be called before it is
    /// defined — and can call itself.
    Call(String, usize),
    Return,
"##;

const COMPILER_FN_EXEC: &str = r##"                Op::Call(name, argc) => {
                    let Some(f) = self.fns.get(name).copied() else {
                        out.push(format!("error: undefined function `{name}`"));
                        break;
                    };
                    if f.arity != *argc {
                        out.push(format!(
                            "error: `{name}` takes {} argument(s), got {argc}",
                            f.arity
                        ));
                        break;
                    }
                    frames.push(Frame {
                        ret: pc,
                        locals: std::collections::HashMap::new(),
                    });
                    pc = f.addr;
                }
                Op::Return => {
                    let frame = frames.pop().expect("return outside a call");
                    pc = frame.ret;
                }
"##;
