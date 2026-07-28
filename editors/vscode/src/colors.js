// @ts-check
//
// Colours for themes that do not style standard scopes.
//
// A TextMate grammar assigns *scopes*; the theme decides what colour a scope
// gets. A theme that only names scopes for its own language — and some do —
// leaves every other language at the default foreground, however correct the
// grammar is. Nothing an extension does to its grammar can change that.
//
// What an extension *can* do is write `editor.tokenColorCustomizations` scoped
// to one theme. That is user-owned, reversible, applies only to `source.nh`
// scopes, and does not fight a theme that already styles them.

const vscode = require("vscode");

/** Dark+ family colours, which most dark themes already use. */
const DARK = {
  comment: "#6A9955",
  string: "#CE9178",
  escape: "#D7BA7D",
  keyword: "#569CD6",
  modifier: "#C586C0",
  ruleName: "#DCDCAA",
  tokenName: "#4EC9B0",
  grammarName: "#4EC9B0",
  label: "#4FC1FF",
  binding: "#9CDCFE",
  preset: "#B5CEA8",
  operator: "#D4D4D4",
  punctuation: "#808080",
};

const LIGHT = {
  comment: "#008000",
  string: "#A31515",
  escape: "#EE0000",
  keyword: "#0000FF",
  modifier: "#AF00DB",
  ruleName: "#795E26",
  tokenName: "#267F99",
  grammarName: "#267F99",
  label: "#0070C1",
  binding: "#001080",
  preset: "#098658",
  operator: "#000000",
  punctuation: "#6E6E6E",
};

/**
 * TextMate rules for every scope the `.nh` grammar produces.
 *
 * Each selector ends in `.nh`, so these cannot affect any other language.
 */
function rules(palette) {
  const p = palette;
  return [
    ["comment.line.double-slash.nh, comment.block.nh", p.comment, "italic"],
    ["string.quoted.double.nh", p.string],
    ["constant.character.escape.nh", p.escape],
    ["invalid.illegal.unknown-escape.nh", "#F44747"],

    ["keyword.control.nh, keyword.control.import.nh", p.keyword, "bold"],
    ["keyword.other.nh", p.keyword],
    ["storage.type.nh", p.keyword],
    ["storage.modifier.nh, storage.modifier.atomic.nh", p.modifier],

    ["entity.name.type.nh", p.grammarName],
    ["entity.name.function.nh", p.ruleName],
    ["entity.name.class.nh", p.tokenName],
    ["entity.name.tag.nh", p.label],

    ["variable.parameter.nh", p.binding],
    ["constant.language.nh", p.preset],
    ["support.class.nh", p.tokenName],
    ["support.function.builtin.nh", p.label, "bold"],

    ["keyword.operator.arrow.nh", p.modifier],
    [
      "keyword.operator.choice.nh, keyword.operator.repetition.nh, " +
        "keyword.operator.lookahead.nh, keyword.operator.range.nh",
      p.operator,
    ],
    // Punctuation stays neutral on purpose. It is structure, not meaning, and
    // a theme that colours it makes a grammar harder to skim, not easier.
    ["punctuation.separator.nh", p.punctuation],
  ].map(([scope, foreground, fontStyle]) => ({
    scope,
    settings: fontStyle ? { foreground, fontStyle } : { foreground },
  }));
}

async function addColors() {
  const theme = vscode.workspace
    .getConfiguration("workbench")
    .get("colorTheme");

  if (!theme) {
    vscode.window.showWarningMessage("No colour theme is set.");
    return;
  }

  const kind = vscode.window.activeColorTheme.kind;
  const palette =
    kind === vscode.ColorThemeKind.Light ||
    kind === vscode.ColorThemeKind.HighContrastLight
      ? LIGHT
      : DARK;

  const config = vscode.workspace.getConfiguration();
  const current =
    /** @type {any} */ (config.get("editor.tokenColorCustomizations")) || {};
  const key = `[${theme}]`;

  // Keep anything already customised for this theme; replace only our rules.
  const existing = current[key] || {};
  const keep = (existing.textMateRules || []).filter(
    (r) => !String(r.scope || "").includes(".nh"),
  );

  const next = {
    ...current,
    [key]: { ...existing, textMateRules: [...keep, ...rules(palette)] },
  };

  await config.update(
    "editor.tokenColorCustomizations",
    next,
    vscode.ConfigurationTarget.Global,
  );

  const choice = await vscode.window.showInformationMessage(
    `Added NailHammer colours for “${theme}”. They apply only to .nh files.`,
    "Show setting",
  );
  if (choice) {
    await vscode.commands.executeCommand(
      "workbench.action.openSettingsJson",
      { revealSetting: { key: "editor.tokenColorCustomizations" } },
    );
  }
}

async function removeColors() {
  const theme = vscode.workspace.getConfiguration("workbench").get("colorTheme");
  const config = vscode.workspace.getConfiguration();
  const current =
    /** @type {any} */ (config.get("editor.tokenColorCustomizations")) || {};
  const key = `[${theme}]`;
  if (!current[key]) return;

  const kept = (current[key].textMateRules || []).filter(
    (r) => !String(r.scope || "").includes(".nh"),
  );

  const next = { ...current };
  if (kept.length === 0 && Object.keys(current[key]).length === 1) {
    delete next[key];
  } else {
    next[key] = { ...current[key], textMateRules: kept };
  }

  await config.update(
    "editor.tokenColorCustomizations",
    next,
    vscode.ConfigurationTarget.Global,
  );
  vscode.window.showInformationMessage(`Removed NailHammer colours for “${theme}”.`);
}

module.exports = { addColors, removeColors, rules, DARK, LIGHT };
