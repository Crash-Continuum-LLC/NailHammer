//! Import resolution (DESIGN.md §3.1).
//!
//! Three behaviours the design calls for, all implemented here:
//!
//!   * **Flat merge.** Definitions from every file land in one namespace. There
//!     is no qualification, because rule references are already a flat
//!     namespace and qualified names would have to thread through tags, view
//!     names, handler module paths, and generated `Rule` variants.
//!   * **Duplicates are hard errors**, never last-wins, and the diagnostic
//!     names both locations.
//!   * **Diamond dedup.** A file reached through two import paths loads once
//!     and is not a duplicate definition. Cycles are an error.
//!
//! Files are collected post-order, so an imported file is reported as the
//! *first* definition and the importing file as the duplicate — which matches
//! how people read the dependency direction.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::ast::*;
use crate::error::{Diagnostic, Errors};
use crate::parse::parse_file;
use crate::source::{FileId, SourceMap, Span, Spanned};

/// Loads `entry`, follows its imports, and returns the merged [`Ast`].
pub fn resolve(sm: &mut SourceMap, entry: &Path) -> Result<Ast, Errors> {
    let mut loader = Loader {
        sm,
        diagnostics: Vec::new(),
        seen: HashMap::new(),
        stack: Vec::new(),
        files: Vec::new(),
    };

    loader.load(entry, None);

    let Loader {
        mut diagnostics,
        files,
        ..
    } = loader;

    let ast = merge(files, &mut diagnostics);

    if diagnostics.iter().any(|d| d.severity == crate::Severity::Error) {
        Err(Errors(diagnostics))
    } else {
        Ok(ast)
    }
}

struct Loader<'a> {
    sm: &'a mut SourceMap,
    diagnostics: Vec<Diagnostic>,
    /// Canonical path → the id it was first loaded under. Dedup keys on the
    /// canonical form so `./a.nh` and `a.nh` are the same file, while the
    /// `SourceMap` keeps the path as written so diagnostics stay readable.
    seen: HashMap<PathBuf, FileId>,
    /// Canonical paths currently being loaded, for cycle detection.
    stack: Vec<PathBuf>,
    files: Vec<Ast>,
}

impl Loader<'_> {
    fn load(&mut self, path: &Path, origin: Option<Span>) {
        let canonical = match std::fs::canonicalize(path) {
            Ok(c) => c,
            Err(e) => {
                self.diagnostics.push(located(
                    Diagnostic::error(format!("cannot read `{}`: {e}", path.display())),
                    origin,
                ));
                return;
            }
        };

        if let Some(pos) = self.stack.iter().position(|p| *p == canonical) {
            let cycle: Vec<String> = self.stack[pos..]
                .iter()
                .chain(std::iter::once(&canonical))
                .map(|p| short(p))
                .collect();
            self.diagnostics.push(located(
                Diagnostic::error("import cycle detected")
                    .note(cycle.join(" -> "), None)
                    .help("remove one of the imports, or move the shared definitions into a third file"),
                origin,
            ));
            return;
        }

        // Diamond: already loaded through another path. Not a duplicate.
        if self.seen.contains_key(&canonical) {
            return;
        }

        let text = match std::fs::read_to_string(&canonical) {
            Ok(t) => t,
            Err(e) => {
                self.diagnostics.push(located(
                    Diagnostic::error(format!("cannot read `{}`: {e}", path.display())),
                    origin,
                ));
                return;
            }
        };

        let id = self.sm.add(path.to_path_buf(), text);
        self.seen.insert(canonical.clone(), id);

        let ast = match parse_file(self.sm, id) {
            Ok(ast) => ast,
            Err(errors) => {
                self.diagnostics.extend(errors.0);
                return;
            }
        };

        self.stack.push(canonical.clone());

        // Join against the path *as written*, not the canonical one, so a
        // relative entry point yields relative paths in diagnostics. Resolution
        // and dedup still go through `canonicalize` inside the recursive call.
        let base = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        for import in &ast.imports {
            let target = base.join(&import.path.value);
            self.load(&target, Some(import.path.span));
        }
        self.stack.pop();

        // Post-order: dependencies are recorded before their importer.
        self.files.push(ast);
    }
}

fn located(d: Diagnostic, span: Option<Span>) -> Diagnostic {
    match span {
        Some(s) => d.at(s),
        None => d,
    }
}

fn short(p: &Path) -> String {
    std::env::current_dir()
        .ok()
        .and_then(|cwd| p.strip_prefix(cwd).ok().map(Path::to_path_buf))
        .unwrap_or_else(|| p.to_path_buf())
        .display()
        .to_string()
}

// ---------------------------------------------------------------------------
// Merging
// ---------------------------------------------------------------------------

fn merge(files: Vec<Ast>, diagnostics: &mut Vec<Diagnostic>) -> Ast {
    let mut out = Ast::default();
    // Name → (kind, span). Tokens, skips, and rules share one namespace,
    // because they all become `.pest` rules and would collide there.
    let mut defined: HashMap<String, (&'static str, Span)> = HashMap::new();

    let mut declare =
        |name: &Spanned<String>, kind: &'static str, diagnostics: &mut Vec<Diagnostic>| -> bool {
            if let Some((prev_kind, prev_span)) = defined.get(&name.value) {
                diagnostics.push(
                    Diagnostic::error(format!("{kind} `{}` already defined", name.value))
                    .at(name.span)
                    .note(
                        format!("first defined here as {} `{}`", prev_kind, name.value),
                        Some(*prev_span),
                    )
                    .help("imports merge into one flat namespace; rename one of them"),
                );
                return false;
            }
            defined.insert(name.value.clone(), (kind, name.span));
            true
        };

    for ast in files {
        let Ast {
            grammar_name,
            imports: _,
            uses,
            keywords_case,
            precedence,
            skips,
            tokens,
            reserved,
            guards,
            boundaries,
            rules,
            recovers,
            expects,
            allows,
        } = ast;

        if let Some(name) = grammar_name {
            match &out.grammar_name {
                Some(prev) => diagnostics.push(
                    Diagnostic::error("more than one `grammar` declaration")
                        .at(name.span)
                        .note("first declared here", Some(prev.span))
                        .help("only the entry file should declare the grammar name"),
                ),
                None => out.grammar_name = Some(name),
            }
        }

        if let Some(mode) = keywords_case {
            match &out.keywords_case {
                Some(prev) if prev.value != mode.value => diagnostics.push(
                    Diagnostic::error("conflicting `keywords` case declarations")
                        .at(mode.span)
                        .note("first declared here", Some(prev.span)),
                ),
                Some(_) => {}
                None => out.keywords_case = Some(mode),
            }
        }

        for s in skips {
            if declare(&s.name, "skip", diagnostics) {
                out.skips.push(s);
            }
        }
        for t in tokens {
            if declare(&t.name, "token", diagnostics) {
                out.tokens.push(t);
            }
        }
        for r in rules {
            if declare(&r.name, "rule", diagnostics) {
                out.rules.push(r);
            }
        }

        out.uses.extend(uses);
        out.precedence.extend(precedence);
        out.reserved.extend(reserved);
        out.guards.extend(guards);
        out.boundaries.extend(boundaries);
        out.recovers.extend(recovers);
        out.expects.extend(expects);
        out.allows.extend(allows);
    }

    if out.grammar_name.is_none() {
        diagnostics.push(
            Diagnostic::error("no `grammar` declaration found")
                .help("add `grammar YourLanguage;` to the entry file"),
        );
    }

    out
}
