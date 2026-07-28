// Tokenises real `.nh` source with the shipped TextMate grammar and asserts
// the scopes.
//
// Syntax highlighting is otherwise unverifiable without looking at it, which
// means a broken rule survives until somebody notices a keyword has gone the
// wrong colour. This runs the same engine VS Code does.
//
//     node test/grammar.test.js

const fs = require("fs");
const path = require("path");
const oniguruma = require("vscode-oniguruma");
const textmate = require("vscode-textmate");

const GRAMMAR = path.join(__dirname, "..", "syntaxes", "nh.tmLanguage.json");

async function registry() {
  const wasm = fs.readFileSync(
    require.resolve("vscode-oniguruma/release/onig.wasm"),
  );
  await oniguruma.loadWASM(wasm.buffer);

  return new textmate.Registry({
    onigLib: Promise.resolve({
      createOnigScanner: (s) => new oniguruma.OnigScanner(s),
      createOnigString: (s) => new oniguruma.OnigString(s),
    }),
    loadGrammar: async (scope) =>
      scope === "source.nh"
        ? textmate.parseRawGrammar(fs.readFileSync(GRAMMAR, "utf8"), GRAMMAR)
        : null,
  });
}

/** Every scope applied to the first occurrence of `needle` in `line`. */
function scopesFor(grammar, line, needle) {
  const result = grammar.tokenizeLine(line, textmate.INITIAL);
  const at = line.indexOf(needle);
  if (at < 0) throw new Error(`\`${needle}\` is not in \`${line}\``);

  const token = result.tokens.find((t) => t.startIndex <= at && at < t.endIndex);
  return token ? token.scopes : [];
}

let failures = 0;

function check(grammar, line, needle, expected) {
  const scopes = scopesFor(grammar, line, needle);
  const ok = scopes.some((s) => s.startsWith(expected));
  if (!ok) {
    failures++;
    console.log(`FAIL  ${JSON.stringify(needle)} in ${JSON.stringify(line)}`);
    console.log(`      expected a scope starting ${expected}`);
    console.log(`      got ${scopes.join(" ")}`);
  } else {
    console.log(`ok    ${JSON.stringify(needle).padEnd(18)} ${expected}`);
  }
}

(async () => {
  const grammar = await (await registry()).loadGrammar("source.nh");
  if (!grammar) throw new Error("the grammar failed to load");

  // Declarations name the thing being defined.
  check(grammar, "grammar Calc;", "grammar", "keyword.control");
  check(grammar, "grammar Calc;", "Calc", "entity.name.type");
  check(grammar, "rule stmt = a;", "rule", "keyword.control");
  check(grammar, "rule stmt = a;", "stmt", "entity.name.function");
  check(grammar, "silent rule item = a | b;", "silent", "storage.modifier");
  check(grammar, "token NUMBER = @ DIGIT+;", "token", "keyword.control");
  check(grammar, "token NUMBER = @ DIGIT+;", "NUMBER", "entity.name.class");
  check(grammar, "skip WHITESPACE = \" \";", "WHITESPACE", "entity.name.class");

  // Comments and strings, including the case-insensitive literal form.
  check(grammar, "// a note", "// a note", "comment.line");
  check(grammar, 'token X = @ "abc";', '"abc"', "string.quoted");
  check(grammar, 'token X = @ ^"abc";', '^"abc"', "string.quoted");
  check(grammar, 'token X = @ "\\n";', "\\n", "constant.character.escape");

  // Bindings become handler parameters, so they read as parameters.
  check(grammar, 'rule s = name:IDENT "=" v:expr;', "name", "variable.parameter");
  check(grammar, "rule s = lazy body:stmt;", "lazy", "storage.modifier");
  check(grammar, "rule s = lazy body:stmt;", "body", "variable.parameter");

  // Labels name handlers; `pass` is not a handler and should look different.
  check(grammar, "rule s = a -> bind;", "bind", "entity.name.tag");
  check(grammar, 'rule s = "(" e:expr ")" -> pass;', "pass", "constant.language");

  // Anchoring is the mistake this project keeps warning about, so SOI/EOI
  // being visibly builtin matters.
  check(grammar, "rule program = SOI stmt* EOI;", "SOI", "support.function.builtin");
  check(grammar, "rule program = SOI stmt* EOI;", "EOI", "support.function.builtin");
  check(grammar, "rule x = !a ANY;", "ANY", "support.function.builtin");

  // The operator table.
  check(grammar, "use operators::core;", "core", "constant.language");
  check(grammar, "precedence override {", "precedence", "keyword.other");
  check(grammar, '    left "+" | "-";', "left", "storage.type");
  check(grammar, '    prefix word "NOT";', "word", "keyword.other");
  check(grammar, "    atom atom;", "atom", "keyword.other");

  // Declarations that are easy to get wrong.
  check(grammar, 'reserved from IDENT { "let" }', "reserved", "keyword.control");
  check(grammar, 'reserved from IDENT { "let" }', "IDENT", "entity.name.class");
  check(grammar, 'guard from IDENT { "atom" }', "guard", "keyword.control");
  check(grammar, "allow unused in file;", "unused", "constant.language");
  check(grammar, 'recover stmt sync ";";', "recover", "keyword.control");
  check(grammar, 'expect ")" in primary as "paren";', "expect", "keyword.control");
  check(grammar, 'import "common.nh";', "import", "keyword.control.import");

  // Expression syntax.
  check(grammar, 'token D = @ "0".."9";', "..", "keyword.operator.range");
  check(grammar, "rule x = a*;", "*", "keyword.operator.repetition");
  check(grammar, "rule x = !a;", "!", "keyword.operator.lookahead");
  check(grammar, "rule x = a | b;", "|", "keyword.operator.choice");

  console.log();
  if (failures > 0) {
    console.log(`${failures} failure(s)`);
    process.exit(1);
  }
  console.log("all scopes as expected");
})();
