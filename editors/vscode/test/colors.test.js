// Every scope the grammar produces must have a colour rule, and every colour
// rule must name a scope the grammar actually produces.
//
// A gap in the first direction is an uncoloured construct — the exact problem
// these rules exist to fix. A gap in the second is a rule for a scope that no
// longer exists, which is dead weight that looks like coverage.
//
//     node test/colors.test.js

const Module = require("module");
const fs = require("fs");
const path = require("path");

const stub = {
  workspace: { getConfiguration: () => ({ get: () => undefined }) },
  window: { activeColorTheme: { kind: 2 } },
  ColorThemeKind: { Light: 1, Dark: 2, HighContrast: 3, HighContrastLight: 4 },
  ConfigurationTarget: { Global: 1 },
  commands: {},
};
const load = Module._load;
Module._load = (req, parent, isMain) =>
  req === "vscode" ? stub : load.apply(Module, [req, parent, isMain]);

const colors = require(path.join(__dirname, "..", "src", "colors.js"));

/** Every `name` in the TextMate grammar, at any depth. */
function grammarScopes() {
  const g = JSON.parse(
    fs.readFileSync(
      path.join(__dirname, "..", "syntaxes", "nh.tmLanguage.json"),
      "utf8",
    ),
  );
  const found = new Set();
  (function walk(node) {
    if (Array.isArray(node)) return node.forEach(walk);
    if (!node || typeof node !== "object") return;
    if (typeof node.name === "string" && node.name.endsWith(".nh")) {
      found.add(node.name);
    }
    for (const [k, v] of Object.entries(node)) {
      if (k === "captures" || k === "beginCaptures" || k === "endCaptures") {
        for (const cap of Object.values(v)) walk(cap);
      } else if (typeof v === "object") {
        walk(v);
      }
    }
  })(g);
  return found;
}

/** Every scope named by a colour rule, split on commas. */
function coloured(palette) {
  const set = new Set();
  for (const rule of colors.rules(palette)) {
    for (const s of rule.scope.split(",")) set.add(s.trim());
  }
  return set;
}

let failures = 0;
const fail = (msg) => {
  failures++;
  console.log(`FAIL  ${msg}`);
};

const scopes = grammarScopes();
const dark = coloured(colors.DARK);
const light = coloured(colors.LIGHT);

console.log(`grammar produces ${scopes.size} scopes; rules cover ${dark.size}\n`);

for (const s of [...scopes].sort()) {
  if (dark.has(s)) console.log(`ok    ${s}`);
  else fail(`${s} has no colour rule — it will render as plain text`);
}

for (const s of [...dark].sort()) {
  if (!scopes.has(s)) fail(`colour rule for \`${s}\`, which the grammar never produces`);
}

// The two palettes must stay in step, or a light-theme user gets gaps.
if (dark.size !== light.size) {
  fail(`dark covers ${dark.size} scopes, light covers ${light.size}`);
}
for (const rule of colors.rules(colors.LIGHT)) {
  if (!/^#[0-9A-Fa-f]{6}$/.test(rule.settings.foreground)) {
    fail(`\`${rule.scope}\` has a malformed colour: ${rule.settings.foreground}`);
  }
}

// Every selector must end in `.nh`, or these rules would recolour other
// languages — which is the thing that makes this safe to write into settings.
for (const s of dark) {
  if (!s.endsWith(".nh")) fail(`\`${s}\` is not scoped to .nh and would affect other languages`);
}

console.log();
if (failures > 0) {
  console.log(`${failures} failure(s)`);
  process.exit(1);
}
console.log("every scope is coloured, and only .nh scopes are touched");
