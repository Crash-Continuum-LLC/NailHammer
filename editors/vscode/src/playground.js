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
    provider.update(asNote("Type a program on the left to see what it routes to."));
    return;
  }

  try {
    const out = await run(["trace", grammar, "--source", source], path.dirname(grammar));
    provider.update(out || asNote("Nothing matched."));
  } catch (e) {
    // `nh trace` exits non-zero for a program that does not parse, and that is
    // the ordinary state of a buffer someone is typing into.
    provider.update(asNote(String(e && e.message ? e.message : e)));
  }
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

  // Untitled, so nothing is written to disk and closing it asks nothing.
  const scratch = await vscode.workspace.openTextDocument({
    content: session?.doc?.getText() ?? "",
  });
  await vscode.window.showTextDocument(scratch, {
    viewColumn: vscode.ViewColumn.One,
    preview: false,
  });

  session = { grammar, doc: scratch };

  const traceDoc = await vscode.workspace.openTextDocument(
    vscode.Uri.parse(`${SCHEME}:${sampleName(grammar)} → handlers`),
  );
  await vscode.window.showTextDocument(traceDoc, {
    viewColumn: vscode.ViewColumn.Two,
    preview: false,
    preserveFocus: true,
  });

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
  context.subscriptions.push(
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
      if (session && doc === session.doc) session = null;
    }),
  );
}

module.exports = { register, sampleName, asNote, SCHEME, TraceProvider };
