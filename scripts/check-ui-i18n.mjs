import fs from "node:fs";
import path from "node:path";
import ts from "typescript";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const english = JSON.parse(
  fs.readFileSync(path.join(root, "support/i18n/en.json")),
);
const traditional = JSON.parse(
  fs.readFileSync(path.join(root, "support/i18n/zh-TW.json")),
);
const errors = [];
const used = new Set();
const slots = (value) =>
  [...value.matchAll(/\{(\w*)\}/g)]
    .map((x) => x[1])
    .sort()
    .join(",");
const han = (value) => /[\p{Script=Han}]/u.test(value);

function scanJs(file, code, kind = ts.ScriptKind.TS) {
  const source = ts.createSourceFile(
    file,
    code,
    ts.ScriptTarget.Latest,
    true,
    kind,
  );
  function visit(node) {
    if (ts.isCallExpression(node) && node.expression.getText(source) === "t") {
      if (node.arguments[0] && ts.isStringLiteral(node.arguments[0])) {
        const key = node.arguments[0].text;
        used.add(key);
        if (!Object.hasOwn(english, key))
          errors.push(`${file}: missing translation: ${key}`);
      }
      for (const argument of node.arguments.slice(1)) visit(argument);
      return;
    }
    if ((ts.isStringLiteral(node) || ts.isJsxText(node)) && han(node.text)) {
      const value = node.text.trim();
      // These two declarations are source messages retained as state; translation
      // happens only when painting the terminal, so switching languages is reversible.
      const stateMessage =
        file.endsWith("/ssh/ui/src/main.ts") &&
        ["正在连接…", "已断开连接"].includes(value);
      if (stateMessage) used.add(value);
      else errors.push(`${file}: hardcoded UI text: ${value}`);
    }
    ts.forEachChild(node, visit);
  }
  visit(source);
}
for (const slug of fs.readdirSync(path.join(root, "official"))) {
  const base = path.join(root, "official", slug, "ui");
  if (!fs.existsSync(base)) continue;
  for (const name of fs.readdirSync(base, { recursive: true })) {
    if (/node_modules|^dist\//.test(name)) continue;
    const file = path.join(base, name);
    if (/\.(ts|tsx)$/.test(name))
      scanJs(
        file,
        fs.readFileSync(file, "utf8"),
        name.endsWith("tsx") ? ts.ScriptKind.TSX : ts.ScriptKind.TS,
      );
    if (!name.endsWith(".html")) continue;
    const html = fs.readFileSync(file, "utf8");
    for (const [, key] of html.matchAll(/data-i18n(?:-[\w-]+)?="([^"]+)"/g)) {
      used.add(key);
      if (!Object.hasOwn(english, key))
        errors.push(`${file}: missing HTML translation: ${key}`);
    }
    for (const [, script] of html.matchAll(
      /<script[^>]*>([\s\S]*?)<\/script>/g,
    ))
      scanJs(file, script, ts.ScriptKind.JS);
  }
}
for (const key of new Set([
  ...Object.keys(english),
  ...Object.keys(traditional),
])) {
  for (const [locale, catalog] of [
    ["en", english],
    ["zh-TW", traditional],
  ]) {
    if (
      typeof catalog[key] !== "string" ||
      !catalog[key] ||
      slots(key) !== slots(catalog[key])
    )
      errors.push(
        `${locale}: missing translation or invalid placeholders: ${key}`,
      );
  }
}
for (const key of JSON.parse(
  execFileSync(
    "python",
    [path.join(root, "scripts/check-native-i18n.py"), "--keys"],
    { encoding: "utf8" },
  ),
))
  used.add(key);
for (const key of Object.keys(english))
  if (!used.has(key)) errors.push(`Unused translation: ${key}`);
if (errors.length) throw new Error(errors.join("\n"));
console.log(
  `UI i18n: ${used.size} source messages; ${Object.keys(english).length} shared UI/native translations verified.`,
);
