// @ts-check
"use strict";

/**
 * The playground's logic, minus the editor.
 *
 * `vscode` only exists inside a running editor, so the module is loaded against
 * a stub. That keeps these runnable in plain node like the rest of the suite —
 * and the type-check (`tsc --noEmit`) is what verifies the real API calls.
 */

const assert = require("assert");
const path = require("path");
const Module = require("module");

// --- stub `vscode` before requiring the module under test -------------------

const fired = [];
const stub = {
  EventEmitter: class {
    constructor() {
      this.event = (fn) => fn;
    }
    fire(x) {
      fired.push(x);
    }
  },
  Uri: {
    parse: (s) => ({ toString: () => s, scheme: s.split(":")[0], path: s.split(":")[1] }),
  },
  Position: class { constructor(l, c) { this.line = l; this.character = c; } },
  WorkspaceEdit: class { insert() {} },
  ViewColumn: { One: 1, Two: 2 },
  workspace: {
    registerTextDocumentContentProvider() {},
    onDidChangeTextDocument() {},
    onDidCloseTextDocument() {},
    openTextDocument: async () => ({ getText: () => "" }),
  },
  window: {
    showTextDocument: async () => {},
    visibleTextEditors: [],
    createStatusBarItem: () => ({ show() {}, hide() {}, dispose() {} }),
  },
  StatusBarAlignment: { Left: 1, Right: 2 },
  commands: { registerCommand() {}, executeCommand: async () => {} },
};

const load = Module._load;
Module._load = function (request, parent, isMain) {
  if (request === "vscode") return stub;
  return load.apply(this, [request, parent, isMain]);
};

const playground = require("../src/playground");

// ---------------------------------------------------------------------------

let failures = 0;
function check(name, fn) {
  try {
    fn();
    console.log(`ok    ${name}`);
  } catch (e) {
    failures += 1;
    console.log(`FAIL  ${name}\n      ${e && e.message}`);
  }
}

// The scratch buffer is named after the grammar, so it picks up whatever
// colouring that language already has.
check("the scratch file is named after the grammar", () => {
  assert.strictEqual(playground.sampleName("/x/mylang.nh"), "playground.mylang");
  assert.strictEqual(playground.sampleName(path.join("a", "b", "calc.nh")), "playground.calc");
});

// Opening over the grammar hides the file you are editing behind the scratch
// buffer you opened to understand it, which is the wrong way round.
check("the panes go beside the grammar, never over it", () => {
  const at1 = playground.columns(1);
  assert.ok(at1.scratch !== 1 && at1.trace !== 1, "column one belongs to the grammar");

  // One column, split top and bottom -- not two more columns. A trace is a deep
  // tree and wants height; three narrow columns gave the wrong dimension to
  // everything.
  // Two groups stacked in one column: `setEditorLayout` numbers them in order,
  // so the rows of the second column are view columns 2 and 3.
  assert.deepStrictEqual(at1, { scratch: 2, trace: 3 });

  // A grammar already in column two pushes the playground right, rather than
  // opening on top of whatever is there.
  assert.deepStrictEqual(playground.columns(2), { scratch: 3, trace: 4 });

  // `activeTextEditor` can be undefined, and `viewColumn` can be a negative
  // sentinel. Neither should produce a nonsense column.
  for (const odd of [undefined, null, -1, -2, 0]) {
    assert.deepStrictEqual(playground.columns(odd), { scratch: 2, trace: 3 }, String(odd));
  }
});

// A real file on disk was never needed for a real tab name.
// splitEditorDown *duplicates* the active editor into the new group, so the
// trace opened as a second tab behind a copy of the program -- and the pane you
// wanted to read was hidden behind one you did not.
check("the layout is set explicitly, never split from under an editor", () => {
  const l = playground.layout(1);
  assert.strictEqual(l.orientation, 0, "grammar beside the playground");
  assert.strictEqual(l.groups.length, 2, "one column for each");
  assert.deepStrictEqual(l.groups[0], {}, "the grammar keeps a plain column");
  assert.strictEqual(l.groups[1].groups.length, 2, "the playground column is two rows");

  // A grammar further right keeps the columns before it intact.
  const l3 = playground.layout(3);
  assert.strictEqual(l3.groups.length, 4);
  assert.ok(l3.groups.slice(0, 3).every((g) => !g.groups), "only the last is split");
});

check("the program buffer is named, and still in memory", () => {
  const uri = playground.scratchUri("/x/mylang.nh");
  assert.strictEqual(uri.scheme, "untitled", "nothing is written to disk");
  assert.ok(
    uri.toString().endsWith("playground.mylang"),
    `the tab should read the language's name, got ${uri.toString()}`,
  );
});

// The complaint that prompted this: two blank panes and no button is a poor
// answer to "how do I run it".
check("the pane says what it is and that it is live", () => {
  const h = playground.header("/x/mylang.nh");
  assert.ok(h.includes("mylang.nh"), "which grammar it traces against");
  assert.ok(/updates as you type/i.test(h), "and that there is nothing to press");
  assert.ok(h.trimEnd().split("\n").length >= 4, "a header, not a word");
});

// Opening onto a working program beats any instruction: the playground is
// already running when it appears.
check("it starts from the project's own sample program", () => {
  const os = require("os");
  const fsx = require("fs");
  const dir = fsx.mkdtempSync(path.join(os.tmpdir(), "nh-seed-"));
  fsx.writeFileSync(path.join(dir, "sample.mylang"), "let a = 1;\n");

  const got = playground.seed(path.join(dir, "mylang.nh"));
  assert.strictEqual(got, "let a = 1;\n");

  // And a project without one still opens, rather than throwing.
  const bare = fsx.mkdtempSync(path.join(os.tmpdir(), "nh-seed-"));
  assert.strictEqual(playground.seed(path.join(bare, "x.nh")), "");
  assert.strictEqual(playground.seed("/nope/does/not/exist/x.nh"), "");
});

check("the virtual document has its own scheme", () => {
  assert.strictEqual(playground.SCHEME, "nailhammer-trace");
  assert.ok(!playground.SCHEME.includes(" "), "a scheme cannot contain a space");
});

// A program mid-edit usually does not parse. Blanking the pane would throw away
// the last good answer exactly when it is most useful to compare against, so a
// failure is rendered as text rather than raised.
check("a failure renders as text rather than throwing", () => {
  const note = playground.asNote("expected `;`");
  assert.ok(note.includes("expected `;`"));
  assert.ok(note.endsWith("\n"), "the pane expects a trailing newline");
});

check("the provider hands back what it was last given", () => {
  const p = new playground.TraceProvider();
  assert.strictEqual(p.provideTextDocumentContent(), "");
  p.update("stmt_bind  → handlers/stmt_bind.rs\n");
  assert.ok(p.provideTextDocumentContent().includes("stmt_bind"));
});

// The event has to name the document that is actually open. Firing a rebuilt
// URI matched nothing, so the pane never re-read and stayed blank.
check("updating fires the change event on the document's own uri", () => {
  const p = new playground.TraceProvider();

  // No URI yet: nothing is open, so nothing should be announced.
  const before = fired.length;
  p.update("x");
  assert.strictEqual(fired.length, before, "fired before a document existed");

  const uri = { toString: () => "nailhammer-trace:playground.basic → handlers" };
  p.uri = uri;
  p.update("y");
  assert.strictEqual(fired.length, before + 1, "no change event fired");
  assert.strictEqual(fired[fired.length - 1], uri, "fired a different uri than was opened");
});

Module._load = load;

if (failures) {
  console.log(`\n${failures} failing`);
  process.exit(1);
}
console.log("\nthe playground's logic holds without an editor");
