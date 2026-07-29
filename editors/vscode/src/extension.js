// @ts-check
//
// The NailHammer extension.
//
// It shells out to the `nh` binary rather than embedding a language server.
// `nh check --json` already produces everything an editor needs — severity,
// range, lint code, help text, and related locations — so a server would add a
// protocol without adding an answer. If completion and go-to-definition arrive
// later, that is when a server earns its place.

const vscode = require("vscode");
const { execFile } = require("child_process");
const path = require("path");
const playground = require("./playground");
const language = require("./language");
const colors = require("./colors");

/** Diagnostics for every `.nh` file we have checked. */
let diagnostics;

/** Debounce timers, one per file, so typing in two files does not interleave. */
const pending = new Map();

/** @type {vscode.OutputChannel} */
let output;

function activate(context) {
  diagnostics = vscode.languages.createDiagnosticCollection("nh");
  output = vscode.window.createOutputChannel("NailHammer");
  context.subscriptions.push(diagnostics, output);

  context.subscriptions.push(
    vscode.commands.registerCommand("nailhammer.newProject", newProject),
    vscode.commands.registerCommand("nailhammer.check", () => checkActive()),
    vscode.commands.registerCommand("nailhammer.build", buildActive),
    vscode.commands.registerCommand("nailhammer.explain", explainActive),
    vscode.commands.registerCommand("nailhammer.addColors", colors.addColors),
    vscode.commands.registerCommand("nailhammer.removeColors", colors.removeColors),
  );

  playground.register(context, activeGrammar, run);

  context.subscriptions.push(
    vscode.workspace.onDidOpenTextDocument(check),
    vscode.workspace.onDidSaveTextDocument(check),
    vscode.workspace.onDidChangeTextDocument((e) => {
      if (config().get("checkOnType")) scheduleCheck(e.document);
    }),
    // A closed file's problems are stale the moment it leaves the editor.
    vscode.workspace.onDidCloseTextDocument((doc) => diagnostics.delete(doc.uri)),
  );

  context.subscriptions.push(
    vscode.tasks.registerTaskProvider("nailhammer", { provideTasks, resolveTask }),
  );

  // Completion, go-to-definition, hover, and the outline.
  language.register(context);

  // Check whatever is already open at startup.
  vscode.workspace.textDocuments.forEach(check);
}

function deactivate() {
  pending.forEach(clearTimeout);
  pending.clear();
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

function scheduleCheck(doc) {
  if (doc.languageId !== "nh") return;
  const key = doc.uri.toString();
  clearTimeout(pending.get(key));
  pending.set(
    key,
    setTimeout(() => {
      pending.delete(key);
      check(doc);
    }, config().get("checkDelay", 400)),
  );
}

async function check(doc) {
  if (doc.languageId !== "nh") return;

  // `nh` reads from disk, so an unsaved buffer is checked through a copy. The
  // alternative is reporting on stale text, which is worse than a temp file.
  const target = doc.isDirty ? await spill(doc) : doc.uri.fsPath;

  const args = ["check", target, "--json", "--quiet"];
  if (config().get("denyWarnings")) args.push("--deny-warnings");

  let stdout;
  try {
    stdout = await run(args, path.dirname(doc.uri.fsPath));
  } catch (e) {
    // A grammar so broken that `nh` could not even start is worth surfacing,
    // but not as a popup on every keystroke.
    output.appendLine(`check failed: ${e.message}`);
    return;
  } finally {
    if (target !== doc.uri.fsPath) cleanup(target);
  }

  let parsed;
  try {
    parsed = JSON.parse(stdout || "[]");
  } catch {
    output.appendLine(`could not parse diagnostics:\n${stdout}`);
    return;
  }

  diagnostics.set(doc.uri, parsed.map((d) => toDiagnostic(d, doc)));
}

/**
 * Converts one `nh` diagnostic into a VS Code one.
 *
 * `nh` reports 1-based line and column, like every compiler; VS Code positions
 * are 0-based, so both drop by one.
 */
function toDiagnostic(d, doc) {
  const range = toRange(d.location, doc);
  const severity =
    d.severity === "error"
      ? vscode.DiagnosticSeverity.Error
      : vscode.DiagnosticSeverity.Warning;

  // The help line is the actionable half of most NailHammer diagnostics, so it
  // belongs in the hover rather than buried in a related-information list.
  const message = d.help ? `${d.message}\n\nhelp: ${d.help}` : d.message;

  const diag = new vscode.Diagnostic(range, message, severity);
  diag.source = "nh";
  if (d.code) diag.code = d.code;

  // A note with a location is the *other* end of a conflict — the earlier
  // alternative that shadows this one, the first definition of a duplicate.
  // Those are exactly what related information is for.
  const related = (d.notes || []).filter((n) => n.location);
  if (related.length > 0) {
    diag.relatedInformation = related.map(
      (n) =>
        new vscode.DiagnosticRelatedInformation(
          new vscode.Location(
            vscode.Uri.file(n.location.file),
            toRange(n.location, doc),
          ),
          n.message,
        ),
    );
  }

  return diag;
}

function toRange(loc, doc) {
  if (!loc) {
    // No span: attach it to the first line rather than dropping it.
    return new vscode.Range(0, 0, 0, 0);
  }
  const start = new vscode.Position(loc.line - 1, loc.column - 1);
  const end = new vscode.Position(loc.endLine - 1, loc.endColumn - 1);
  // A zero-width range is invisible. Widen it to the rest of the line so the
  // squiggle is findable.
  if (start.isEqual(end) && doc) {
    return doc.lineAt(Math.min(start.line, doc.lineCount - 1)).range;
  }
  return new vscode.Range(start, end);
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

async function newProject() {
  const name = await vscode.window.showInputBox({
    title: "New NailHammer project",
    prompt: "Project name — also the crate name",
    placeHolder: "mylang",
    validateInput: (v) =>
      /^[A-Za-z][A-Za-z0-9_-]*$/.test(v)
        ? null
        : "Letters, digits, `-` and `_`, starting with a letter.",
  });
  if (!name) return;

  const ext = await vscode.window.showInputBox({
    title: "New NailHammer project",
    prompt: "File extension for your language's source files",
    value: name,
    validateInput: (v) =>
      /^[A-Za-z0-9]+$/.test(v) ? null : "Letters and digits only, with no dot.",
  });
  if (!ext) return;

  const picked = await vscode.window.showOpenDialog({
    title: "Where should the project go?",
    canSelectFolders: true,
    canSelectFiles: false,
    openLabel: "Create here",
  });
  if (!picked || picked.length === 0) return;

  const dir = path.join(picked[0].fsPath, name);

  try {
    await run(["init", dir, "--name", name, "--ext", ext], picked[0].fsPath);
  } catch (e) {
    vscode.window.showErrorMessage(`nh init failed: ${e.message}`);
    return;
  }

  const open = await vscode.window.showInformationMessage(
    `Created ${name}. It builds and runs as-is.`,
    "Open",
    "Open in New Window",
  );
  if (open) {
    await vscode.commands.executeCommand(
      "vscode.openFolder",
      vscode.Uri.file(dir),
      { forceNewWindow: open === "Open in New Window" },
    );
  }
}

async function checkActive() {
  const doc = activeGrammar();
  if (doc) await check(doc);
}

async function buildActive() {
  const doc = activeGrammar();
  if (!doc) return;
  if (doc.isDirty) await doc.save();

  const dir = path.dirname(doc.uri.fsPath);
  const base = path.basename(doc.uri.fsPath, ".nh");
  const out = config().get("rustOutDir", "src");

  try {
    const stdout = await run(
      [
        "build",
        doc.uri.fsPath,
        "-o",
        path.join(dir, out, `${base}.pest`),
        "--rust",
        path.join(dir, out),
      ],
      dir,
    );
    output.appendLine(stdout.trim());
    output.show(true);
  } catch (e) {
    output.appendLine(e.message);
    output.show(true);
    vscode.window.showErrorMessage("nh build failed — see the NailHammer output.");
  }
}

async function explainActive() {
  const doc = activeGrammar();
  if (!doc) return;
  if (doc.isDirty) await doc.save();

  try {
    const table = await run(["explain", doc.uri.fsPath], path.dirname(doc.uri.fsPath));
    const shown = await vscode.workspace.openTextDocument({
      content: table,
      language: "plaintext",
    });
    await vscode.window.showTextDocument(shown, { preview: true });
  } catch (e) {
    vscode.window.showErrorMessage(`nh explain failed: ${e.message}`);
  }
}

function activeGrammar() {
  const editor = vscode.window.activeTextEditor;
  if (!editor || editor.document.languageId !== "nh") {
    vscode.window.showWarningMessage("Open a `.nh` grammar first.");
    return null;
  }
  return editor.document;
}

// ---------------------------------------------------------------------------
// Tasks
// ---------------------------------------------------------------------------

function provideTasks() {
  const folder = vscode.workspace.workspaceFolders?.[0];
  if (!folder) return [];
  return ["check", "build"].map((command) => makeTask({ type: "nailhammer", command }, folder));
}

function resolveTask(task) {
  const folder = vscode.workspace.workspaceFolders?.[0];
  return folder ? makeTask(task.definition, folder, task) : undefined;
}

function makeTask(definition, folder, existing) {
  const file = definition.file || "${file}";
  const args = definition.command === "build" ? ["build", file, "--rust", "src"] : ["check", file];

  return new vscode.Task(
    definition,
    folder,
    `nh ${definition.command}`,
    "nailhammer",
    new vscode.ShellExecution(exe(), args),
    "$nh",
  );
}

// ---------------------------------------------------------------------------
// Running `nh`
// ---------------------------------------------------------------------------

function config() {
  return vscode.workspace.getConfiguration("nailhammer");
}

/** The configured binary, with `${workspaceFolder}` expanded. */
function exe() {
  const raw = config().get("executable", "nh");
  const folder = vscode.workspace.workspaceFolders?.[0];
  return folder ? raw.replace("${workspaceFolder}", folder.uri.fsPath) : raw;
}

function run(args, cwd) {
  return new Promise((resolve, reject) => {
    execFile(exe(), args, { cwd }, (err, stdout, stderr) => {
      // `nh check` exits non-zero when the grammar has errors, and those
      // errors *are* the output. Only a missing binary is a real failure.
      if (err && /** @type {any} */ (err).code === "ENOENT") {
        reject(
          new Error(
            `cannot run \`${exe()}\`. Set \`nailhammer.executable\` to the built binary, ` +
              `for example \${workspaceFolder}/target/debug/nh`,
          ),
        );
        return;
      }
      if (err && !stdout) {
        reject(new Error(stderr || err.message));
        return;
      }
      resolve(stdout);
    });
  });
}

// ---------------------------------------------------------------------------
// Unsaved buffers
// ---------------------------------------------------------------------------

const fs = require("fs");
const os = require("os");

/**
 * Writes an unsaved buffer beside the real file.
 *
 * Beside it, not in the temp directory, because `import` paths are resolved
 * relative to the importing file — a copy somewhere else would fail to find
 * everything the grammar imports.
 */
async function spill(doc) {
  const dir = path.dirname(doc.uri.fsPath);
  const name = `.${path.basename(doc.uri.fsPath, ".nh")}.nh-check.nh`;
  const target = path.join(dir, name);
  try {
    await fs.promises.writeFile(target, doc.getText());
    return target;
  } catch {
    // A read-only directory is not worth failing over; check the saved text.
    const fallback = path.join(os.tmpdir(), name);
    await fs.promises.writeFile(fallback, doc.getText());
    return fallback;
  }
}

function cleanup(target) {
  fs.promises.unlink(target).catch(() => {});
}

module.exports = { activate, deactivate };
