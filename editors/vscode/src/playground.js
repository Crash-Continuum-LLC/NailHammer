// @ts-check
"use strict";

/**
 * The evaluation playground.
 *
 * Two panes: a scratch buffer you type a program into, and a live view of what
 * that program routes to — which handler, with which arguments, and which of
 * them have not been evaluated yet.
 *
 * It is driven by `nh trace`, which interprets the grammar rather than
 * generating and compiling Rust, so the answer arrives as fast as parsing. That
 * is what makes typing into it worthwhile: without it, every keystroke would
 * cost a cargo build.
 */

const vscode = require("vscode");
const path = require("path");
const fs = require("fs");

/** The virtual document the trace is shown in. */
const SCHEME = "nailhammer-trace";

/**
 * One playground: the grammar it traces against, the buffer being traced, and
 * the last result.
 *
 * There is at most one. A second would need its own URI, and two live traces
 * competing for the same `nh` is not worth the complexity for a scratch tool.
 */
let session = null;

/** Debounce, so a burst of keystrokes runs `nh trace` once. */
let pending = null;

/**
 * Content for the trace pane.
 *
 * A `TextDocumentContentProvider` rather than a webview: the output is text,
 * the editor already renders text well, and it stays selectable, searchable and
 * copyable for free.
 */
class TraceProvider {
  constructor() {
    this.emitter = new vscode.EventEmitter();
    this.onDidChange = this.emitter.event;
    this.body = "";
  }

  provideTextDocumentContent() {
    return this.body;
  }

  update(body) {
    this.body = body;
    this.emitter.fire(vscode.Uri.parse(`${SCHEME}:trace`));
  }
}

const provider = new TraceProvider();

/**
 * The scratch file's extension, from the grammar's own name.
 *
 * `mylang.nh` gets `sample.mylang`, matching what `nh init` writes — so the
 * buffer opens with whatever colouring that language already has, if any.
 */
function sampleName(grammarPath) {
  const stem = path.basename(grammarPath, ".nh");
  return `playground.${stem}`;
}

/**
 * The header the trace pane always carries.
 *
 * The first version of this shipped two blank panes and no button, which is a
 * poor answer to "how do I run it". Nothing here is decoration: it says what
 * the pane is, which grammar it is tracing against, and — the part that was
 * missing — that there is nothing to press.
 */
function header(grammar) {
  const name = path.basename(grammar);
  return [
    `NailHammer — evaluation playground`,
    `grammar: ${name}`,
    ``,
    `Edit the program above. This updates as you type — or press the ▶ button`,
    `in its tab bar, or ${process.platform === "darwin" ? "Cmd" : "Ctrl"}+Enter.`,
    `─`.repeat(64),
    ``,
  ].join("\n");
}

/**
 * What to put in the scratch buffer when the playground first opens.
 *
 * A project scaffolded by `nh init` has a `sample.<ext>` beside its grammar,
 * and it is a working program in the language. Starting from it means the
 * playground is *already running* when it opens — which answers "how do I use
 * this" better than any instruction could.
 */
function seed(grammar) {
  const dir = path.dirname(grammar);
  try {
    const sample = fs.readdirSync(dir).find((f) => f.startsWith("sample."));
    if (sample) return fs.readFileSync(path.join(dir, sample), "utf8");
  } catch {
    // No directory, no sample, no matter — fall through to the note.
  }
  return "";
}

/**
 * Renders a failure the same way a trace is rendered, so the pane never goes
 * blank while you are mid-edit.
 *
 * A program that does not parse yet is the normal state of a buffer being
 * typed into — it is not worth a popup, and blanking the pane loses the last
 * good answer just when it is most useful to compare against.
 */
function asNote(message) {
  return `${message}\n`;
}

/**
 * Runs `nh trace` for the current buffer and pushes the result to the pane.
 *
 * @param {(args: string[], cwd: string) => Promise<string>} run
 */
async function refresh(run) {
  if (!session) return;
  const { grammar, doc } = session;
  const source = doc.getText();

  if (!source.trim()) {
    provider.update(
      header(grammar) +
        asNote(
          "Nothing to trace yet.\n\n" +
            "Type a program in the pane above — a line of the language your\n" +
            "grammar describes, not the grammar itself. This pane will show\n" +
            "which handler each part of it reaches, and what gets passed.",
        ),
    );
    status(grammar, "empty");
    return;
  }

  try {
    const out = await run(["trace", grammar, "--source", source], path.dirname(grammar));
    provider.update(header(grammar) + (out || asNote("Nothing matched.")));
    status(grammar, "ok");
  } catch (e) {
    // `nh trace` exits non-zero for a program that does not parse, and that is
    // the ordinary state of a buffer someone is typing into.
    provider.update(header(grammar) + asNote(String(e && e.message ? e.message : e)));
    status(grammar, "error");
  }
}

/**
 * Which column the playground occupies, given where the grammar is.
 *
 * **Not column one.** That is where the grammar usually is, and opening over it
 * hides the file you are editing behind the buffer you opened to understand it
 * — which is the wrong way round.
 *
 * One column, not two: the program and its trace share it, split top and
 * bottom. A trace is a deep tree, so it wants height; a program is usually a
 * few lines. Three narrow columns gave the wrong dimension to both.
 */
function columns(grammarColumn) {
  const at = typeof grammarColumn === "number" && grammarColumn > 0 ? grammarColumn : 1;
  return { scratch: at + 1, trace: at + 2 };
}

/**
 * An editor layout with the playground stacked beside the grammar.
 *
 * Set explicitly rather than by `workbench.action.splitEditorDown`, which
 * *duplicates* the active editor into the new group — the trace then opened as
 * a second tab behind a copy of the program, and the pane you wanted to read
 * was hidden behind one you did not.
 *
 * Orientation 0 is side by side; the nested pair is the column split into two
 * rows. Columns are numbered in order, so the grammar keeps its own and the
 * playground gets the two after it.
 */
function layout(grammarColumn) {
  const before = Array.from({ length: Math.max(0, grammarColumn - 1) }, () => ({}));
  return {
    orientation: 0,
    groups: [...before, {}, { groups: [{}, {}] }],
  };
}

/**
 * The URI of the in-memory program buffer.
 *
 * `untitled:` with a path, rather than an anonymous buffer: the tab then reads
 * `playground.mylang` instead of `Untitled-1`, and the extension gives it
 * whatever language mode that suffix is registered for. Nothing is written to
 * disk either way — a real file was never needed for a real name.
 */
function scratchUri(grammar) {
  return vscode.Uri.parse(`untitled:${sampleName(grammar)}`);
}

/**
 * Opens the playground for the active grammar.
 *
 * @param {() => vscode.TextDocument | undefined} activeGrammar
 * @param {(args: string[], cwd: string) => Promise<string>} run
 */
async function open(activeGrammar, run) {
  const grammarDoc = activeGrammar();
  if (!grammarDoc) return;
  if (grammarDoc.isDirty) await grammarDoc.save();

  const grammar = grammarDoc.uri.fsPath;
  const where = columns(vscode.window.activeTextEditor?.viewColumn);

  // Reuse the buffer if one is already open. Running the command twice used to
  // leave a second untitled document behind, and only the newest was traced.
  const uri = scratchUri(grammar);
  const scratch = await vscode.workspace.openTextDocument(uri);

  // Seeded from the project's sample, so the playground opens already running.
  if (!scratch.getText()) {
    const text = seed(grammar);
    if (text) {
      const edit = new vscode.WorkspaceEdit();
      edit.insert(uri, new vscode.Position(0, 0), text);
      await vscode.workspace.applyEdit(edit);
    }
  }

  session = { grammar, doc: scratch };

  const traceDoc = await vscode.workspace.openTextDocument(
    vscode.Uri.parse(`${SCHEME}:${sampleName(grammar)} → handlers`),
  );

  // The layout first, so both panes land in groups that already exist and
  // neither has to be split out from under the other.
  await vscode.commands.executeCommand(
    "vscode.setEditorLayout",
    layout(where.scratch - 1),
  );

  await vscode.window.showTextDocument(traceDoc, {
    viewColumn: where.trace,
    preview: false,
    preserveFocus: true,
  });
  // Last, so focus ends where you type.
  await vscode.window.showTextDocument(scratch, {
    viewColumn: where.scratch,
    preview: false,
  });

  // Visible immediately, rather than only once a trace has run — the point of
  // it is to say the playground is live.
  status(grammar, "empty");
  await refresh(run);
}

/**
 * Wires the playground into the extension.
 *
 * @param {vscode.ExtensionContext} context
 * @param {() => vscode.TextDocument | undefined} activeGrammar
 * @param {(args: string[], cwd: string) => Promise<string>} run
 */
function register(context, activeGrammar, run) {
  bar = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 100);
  bar.command = "nailhammer.traceNow";

  context.subscriptions.push(
    bar,
    vscode.commands.registerCommand("nailhammer.traceNow", () => refresh(run)),
    vscode.workspace.registerTextDocumentContentProvider(SCHEME, provider),
    vscode.commands.registerCommand("nailhammer.playground", () =>
      open(activeGrammar, run),
    ),
    vscode.workspace.onDidChangeTextDocument((e) => {
      if (!session || e.document !== session.doc) return;
      if (pending) clearTimeout(pending);
      pending = setTimeout(() => refresh(run), 200);
    }),
    // A closed scratch buffer ends the session; leaving it live would keep
    // tracing a document nobody can see.
    vscode.workspace.onDidCloseTextDocument((doc) => {
      if (session && doc === session.doc) {
        session = null;
        bar?.hide();
      }
    }),
  );
}

/**
 * A status bar item, so there is something visible that says this is live —
 * and something to click when you want it run again.
 */
let bar = null;

function status(grammar, state) {
  if (!bar) return;
  const name = path.basename(grammar);
  const face = {
    ok: `$(check) nh: ${name}`,
    error: `$(warning) nh: ${name}`,
    empty: `$(circle-outline) nh: ${name}`,
  };
  bar.text = face[state] || face.empty;
  bar.tooltip =
    state === "error"
      ? "That program does not parse yet. Click to trace again."
      : "NailHammer playground — updates as you type. Click to trace again.";
  bar.show();
}

module.exports = {
  register,
  sampleName,
  asNote,
  columns,
  header,
  seed,
  scratchUri,
  layout,
  SCHEME,
  TraceProvider,
};
