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
  Uri: { parse: (s) => ({ toString: () => s, scheme: s.split(":")[0] }) },
  ViewColumn: { One: 1, Two: 2 },
  workspace: {
    registerTextDocumentContentProvider() {},
    onDidChangeTextDocument() {},
    onDidCloseTextDocument() {},
    openTextDocument: async () => ({ getText: () => "" }),
  },
  window: { showTextDocument: async () => {} },
  commands: { registerCommand() {} },
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

check("updating tells the editor to re-read", () => {
  const before = fired.length;
  new playground.TraceProvider().update("x");
  assert.strictEqual(fired.length, before + 1, "no change event fired");
});

Module._load = load;

if (failures) {
  console.log(`\n${failures} failing`);
  process.exit(1);
}
console.log("\nthe playground's logic holds without an editor");
