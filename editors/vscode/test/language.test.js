// Tests the completion/definition logic without an editor.
//
// `language.js` requires `vscode`, which only exists inside the extension host,
// so a stub is injected into the module cache first. Everything under test is
// pure: indexing a document and deciding what context the cursor is in.
//
//     node test/language.test.js

const Module = require("module");
const path = require("path");

// --- the stub --------------------------------------------------------------

class Position {
  constructor(line, character) {
    this.line = line;
    this.character = character;
  }
}
const enumOf = (...names) =>
  Object.fromEntries(names.map((n, i) => [n, i]));

const stub = {
  Position,
  Range: class {},
  Location: class {},
  Hover: class {},
  SymbolInformation: class {},
  MarkdownString: class {
    appendCodeblock() {
      return this;
    }
  },
  SnippetString: class {},
  CompletionItem: class {
    constructor(label, kind) {
      this.label = label;
      this.kind = kind;
    }
  },
  CompletionItemKind: enumOf("Keyword", "Value", "Method", "Function", "Class", "Constant"),
  SymbolKind: enumOf("Function", "Class"),
  languages: {
    registerCompletionItemProvider() {},
    registerDefinitionProvider() {},
    registerHoverProvider() {},
    registerDocumentSymbolProvider() {},
  },
};

const load = Module._load;
Module._load = function (request, parent, isMain) {
  if (request === "vscode") return stub;
  return load.apply(this, [request, parent, isMain]);
};

const language = require(path.join(__dirname, "..", "src", "language.js"));

// --- a fake TextDocument ---------------------------------------------------

function doc(source) {
  const lines = source.split("\n");
  return {
    lineCount: lines.length,
    lineAt: (i) => ({ text: lines[i] }),
    getText: () => source,
  };
}

// --- assertions ------------------------------------------------------------

let failures = 0;
function eq(actual, expected, what) {
  const a = JSON.stringify(actual);
  const b = JSON.stringify(expected);
  if (a !== b) {
    failures++;
    console.log(`FAIL  ${what}\n      expected ${b}\n      got      ${a}`);
  } else {
    console.log(`ok    ${what}`);
  }
}

const GRAMMAR = `grammar Calc;

use operators::core;

skip WHITESPACE = " ";
token NUMBER = @ DIGIT+;
silent rule item = a | b;
rule program = SOI stmt* EOI;
rule stmt
  = "let" name:IDENT "=" value:expr ";" -> bind
  ;
// rule commented = nope;
`;

// Indexing finds every definition, and only real ones.
const names = language.index(doc(GRAMMAR)).map((d) => `${d.kind}:${d.name}`);
eq(names, ["skip:WHITESPACE", "token:NUMBER", "rule:item", "rule:program", "rule:stmt"],
  "index finds rules, tokens and skips");
eq(names.includes("rule:commented"), false, "a commented-out definition is not indexed");

// `silent rule x` is still a rule named `x`, not one named `silent`.
eq(language.index(doc("silent rule item = a;"))[0].name, "item",
  "`silent rule` indexes the rule name");

// Context decides what is worth offering.
// The cursor defaults to the end of the line, which is where completion is
// actually requested. Passing a column by hand is how the first draft of this
// test asked about column 0 and "proved" the wrong answer.
const at = (src, line = 0, ch) =>
  language.contextAt(
    doc(src),
    new Position(line, ch ?? src.split("\n")[line].length),
  );

eq(at("use operators::"), "preset", "after `use operators::` offer presets");
eq(at("use operators::c"), "preset", "...and partway through a preset name");
eq(at("allow "), "lint", "after `allow` offer lint names");
eq(at("rule s = a -> "), "role-or-label", "after `->` offer roles and `pass`");
eq(at("recover "), "rule-name", "after `recover` offer rule names");
eq(at("reserved from "), "token-name", "after `reserved from` offer token names");
eq(at("gram"), "declaration", "at the start of a line offer declarations");
eq(at("rule x = SOI st"), "expression", "mid-expression offer rules, tokens, builtins");

// Inside a precedence block the vocabulary is fixity words, not declarations.
const TABLE = `precedence {
    left "+" | "-";

}
rule x = a;`;
eq(at(TABLE, 2), "precedence", "inside `precedence { }` offer fixity words");
eq(at(TABLE, 4), "expression", "after the block closes, back to expressions");

// The vocabulary tables must match the tool. A drifted list is worse than none,
// because it suggests things `nh check` will reject.
eq(language.LINTS.length, 8, "one entry per lint the CLI reports");
eq(language.PRESETS.map((p) => p[0]), ["core", "c_style", "c_strict", "none"],
  "presets match `use operators::`");
eq(language.BUILTINS.map((b) => b[0]).includes("SOI"), true, "SOI is offered");

console.log();
if (failures > 0) {
  console.log(`${failures} failure(s)`);
  process.exit(1);
}
console.log("language features behave");
