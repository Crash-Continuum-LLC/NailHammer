// @ts-check
//
// Completion, go-to-definition, hover, and the outline.
//
// All of it runs off a regex index of the open document. `.nh` declarations are
// one line each and start with a keyword, so a parser would buy nothing here
// that a scan does not — and a scan keeps working while the file is mid-edit
// and does not parse, which is exactly when completion is wanted.

const vscode = require("vscode");

// ---------------------------------------------------------------------------
// Vocabulary
// ---------------------------------------------------------------------------

/** Keywords that begin a declaration, with what they are for. */
const DECLARATIONS = [
  ["grammar", "Name this language. Exactly one across all imported files."],
  ["import", "Merge another `.nh` file. Paths are relative to this file."],
  ["use", "Take an operator preset: `use operators::core;`"],
  ["keywords", "`keywords case-insensitive;` folds literals and word operators."],
  ["precedence", "Declare the operator table."],
  ["skip", "Trivia matched implicitly between elements."],
  ["token", "A lexical token. `@` makes it atomic."],
  ["reserved", "Boundary-guard keywords *and* reject them as identifiers."],
  ["guard", "Boundary-guard without reserving — a word that is also a valid name."],
  ["boundary", "State what may follow a token, when it cannot be derived."],
  ["rule", "A grammar rule. Each labelled alternative gets one handler."],
  ["silent", "`silent rule` matches but produces no node. It cannot be bound."],
  ["recover", "Resynchronise so one bad statement does not hide the rest."],
  ["expect", "Replace a rule-name parse error with a sentence."],
  ["allow", "Silence one lint in one rule."],
];

const PRESETS = [
  ["core", "Arithmetic, comparison, and short-circuiting. The usual starting point."],
  ["c_style", "The full C operator set, with C's bitwise/comparison defect corrected."],
  ["c_strict", "C exactly, defect included."],
  ["none", "Empty. Write the table yourself."],
];

const LINTS = [
  ["left-recursion", "a rule that can reach itself without consuming input"],
  ["nullable-repetition", "a repetition whose body can match nothing"],
  ["shadow", "an earlier alternative that makes a later one unreachable"],
  ["unreachable-alternative", "an alternative after one that always matches"],
  ["duplicate-binding", "the same binding name twice in one sequence"],
  ["unused", "a rule or token nothing refers to"],
  ["recover-sync", "a `recover` sync point that can match nothing"],
  ["silent-binding", "a binding onto a rule that produces no node"],
];

const BUILTINS = [
  ["SOI", "Start of input. Anchors the entry rule — without it a program that opens with trivia will not parse."],
  ["EOI", "End of input."],
  ["ANY", "Any single character."],
  ["NEWLINE", "A line ending."],
  ["PUSH", "Push onto pest's stack."],
  ["POP", "Pop pest's stack."],
  ["PEEK", "Read pest's stack."],
  ["DROP", "Discard the top of pest's stack."],
];

const FIXITY = [
  ["left", "Left-associative infix."],
  ["right", "Right-associative infix."],
  ["prefix", "Prefix operator."],
  ["postfix", "Postfix operator."],
  ["atom", "`atom NAME;` names the rule the driver builds expressions from."],
  ["remove", "Drop operators inherited from a preset."],
  ["word", "An identifier-shaped operator. Reserved automatically."],
  ["above", "Bind tighter than a named operator."],
  ["below", "Bind looser than a named operator."],
  ["lazy", "`lazy(rhs)` overrides a role's default laziness."],
];

/** Roles the generated `Operators` trait knows how to name a method for. */
const ROLES = [
  "add", "sub", "mul", "div", "rem", "pow", "neg", "pos",
  "compare", "eq", "ne", "lt", "le", "gt", "ge",
  "and_then", "or_else", "not", "coalesce", "ternary",
  "bit_and", "bit_or", "bit_xor", "bit_not", "shl", "shr",
  "assign", "compound_assign", "concat", "range", "inc", "dec",
];

// ---------------------------------------------------------------------------
// Indexing a document
// ---------------------------------------------------------------------------

/**
 * Every rule and token the document defines.
 *
 * @returns {{name: string, kind: "rule"|"token"|"skip", line: number, text: string}[]}
 */
function index(doc) {
  const out = [];
  const re = /^\s*(?:silent\s+)?(rule|token|skip|boundary)\s+([A-Za-z_][A-Za-z0-9_]*)/;

  for (let i = 0; i < doc.lineCount; i++) {
    const line = doc.lineAt(i).text;
    if (line.trimStart().startsWith("//")) continue;
    const m = re.exec(line);
    if (m) {
      out.push({
        name: m[2],
        kind: /** @type {any} */ (m[1] === "boundary" ? "token" : m[1]),
        line: i,
        text: definitionText(doc, i),
      });
    }
  }
  return out;
}

/** A definition, joined across the lines it spans, for hovers. */
function definitionText(doc, start) {
  const lines = [];
  for (let i = start; i < doc.lineCount && i < start + 12; i++) {
    const text = doc.lineAt(i).text;
    lines.push(text);
    if (text.includes(";")) break;
  }
  return lines.join("\n").trim();
}

/** Where we are, which decides what is worth suggesting. */
function contextAt(doc, position) {
  const line = doc.lineAt(position.line).text;
  const before = line.slice(0, position.character);

  if (/\buse\s+operators\s*::\s*[A-Za-z_]*$/.test(before)) return "preset";
  if (/\ballow\s+[a-z-]*$/.test(before)) return "lint";
  if (/->\s*[A-Za-z_]*$/.test(before)) return "role-or-label";
  if (/\b(recover|expect.*\bin)\s+[A-Za-z_]*$/.test(before)) return "rule-name";
  if (/\b(reserved|guard)\s+from\s+[A-Za-z_]*$/.test(before)) return "token-name";

  // Inside a `precedence { .. }` block, fixity words are the vocabulary.
  let depth = 0;
  for (let i = position.line; i >= 0; i--) {
    const text = doc.lineAt(i).text;
    if (i < position.line) depth += (text.match(/\}/g) || []).length;
    if (/\bprecedence\b/.test(text) && depth === 0) return "precedence";
    if (i < position.line) depth -= (text.match(/\{/g) || []).length;
    if (depth < 0) break;
  }

  // A declaration keyword only makes sense at the start of a line.
  if (/^\s*[A-Za-z_]*$/.test(before)) return "declaration";

  return "expression";
}

// ---------------------------------------------------------------------------
// Providers
// ---------------------------------------------------------------------------

const completion = {
  provideCompletionItems(doc, position) {
    const where = contextAt(doc, position);
    const items = [];

    const add = (label, detail, kind, doc_) => {
      const item = new vscode.CompletionItem(label, kind);
      item.detail = detail;
      if (doc_) item.documentation = new vscode.MarkdownString(doc_);
      items.push(item);
      return item;
    };

    switch (where) {
      case "preset":
        for (const [name, why] of PRESETS)
          add(name, "operator preset", vscode.CompletionItemKind.Value, why);
        return items;

      case "lint":
        for (const [name, why] of LINTS)
          add(name, why, vscode.CompletionItemKind.Value);
        return items;

      case "role-or-label":
        add("pass", "transparent — no handler is generated", vscode.CompletionItemKind.Keyword,
          "The alternative evaluates to whatever its single child does.");
        for (const role of ROLES)
          add(role, "operator role", vscode.CompletionItemKind.Method,
            "Names the generated `Operators` method. Roles are about meaning, not spelling.");
        return items;

      case "precedence":
        for (const [name, why] of FIXITY)
          add(name, why, vscode.CompletionItemKind.Keyword);
        for (const role of ROLES)
          add(role, "operator role", vscode.CompletionItemKind.Method);
        return items;

      case "declaration":
        for (const [name, why] of DECLARATIONS)
          add(name, "declaration", vscode.CompletionItemKind.Keyword, why);
        return items;

      case "rule-name":
        for (const d of index(doc).filter((d) => d.kind === "rule"))
          add(d.name, "rule", vscode.CompletionItemKind.Function);
        return items;

      case "token-name":
        for (const d of index(doc).filter((d) => d.kind === "token"))
          add(d.name, "token", vscode.CompletionItemKind.Class);
        return items;

      default: {
        // In an expression: everything you could reference, plus the builtins.
        for (const d of index(doc)) {
          add(
            d.name,
            d.kind,
            d.kind === "rule"
              ? vscode.CompletionItemKind.Function
              : vscode.CompletionItemKind.Class,
            "```nh\n" + d.text + "\n```",
          );
        }
        for (const [name, why] of BUILTINS)
          add(name, "pest builtin", vscode.CompletionItemKind.Constant, why);

        const lazy = add("lazy", "hand the handler the node, not the value",
          vscode.CompletionItemKind.Keyword,
          "The handler receives it unevaluated, so it can decline to run it — or keep it.");
        lazy.insertText = new vscode.SnippetString("lazy ${1:name}:${2:rule}");
        return items;
      }
    }
  },
};

const definition = {
  provideDefinition(doc, position) {
    const range = doc.getWordRangeAtPosition(position, /[A-Za-z_][A-Za-z0-9_]*/);
    if (!range) return null;
    const word = doc.getText(range);

    const hit = index(doc).find((d) => d.name === word);
    return hit
      ? new vscode.Location(doc.uri, new vscode.Position(hit.line, 0))
      : null;
  },
};

const hover = {
  provideHover(doc, position) {
    const range = doc.getWordRangeAtPosition(position, /[A-Za-z_][A-Za-z0-9_-]*/);
    if (!range) return null;
    const word = doc.getText(range);

    const hit = index(doc).find((d) => d.name === word);
    if (hit) {
      return new vscode.Hover(
        new vscode.MarkdownString().appendCodeblock(hit.text, "nh"),
        range,
      );
    }

    for (const [table, label] of [
      [BUILTINS, "pest builtin"],
      [LINTS, "lint"],
      [PRESETS, "operator preset"],
      [DECLARATIONS, "declaration"],
      [FIXITY, "operator table"],
    ]) {
      const found = /** @type {string[][]} */ (table).find((e) => e[0] === word);
      if (found) {
        return new vscode.Hover(
          new vscode.MarkdownString(`**${word}** — *${label}*\n\n${found[1]}`),
          range,
        );
      }
    }
    return null;
  },
};

const symbols = {
  provideDocumentSymbols(doc) {
    return index(doc).map(
      (d) =>
        new vscode.SymbolInformation(
          d.name,
          d.kind === "rule"
            ? vscode.SymbolKind.Function
            : vscode.SymbolKind.Class,
          d.kind,
          new vscode.Location(doc.uri, new vscode.Position(d.line, 0)),
        ),
    );
  },
};

function register(context) {
  const nh = { language: "nh" };
  context.subscriptions.push(
    // `:` and `>` so `name:` and `->` trigger without a keystroke after them.
    vscode.languages.registerCompletionItemProvider(nh, completion, ":", ">", " "),
    vscode.languages.registerDefinitionProvider(nh, definition),
    vscode.languages.registerHoverProvider(nh, hover),
    vscode.languages.registerDocumentSymbolProvider(nh, symbols),
  );
}

module.exports = { register, index, contextAt, DECLARATIONS, LINTS, PRESETS, ROLES, BUILTINS };
