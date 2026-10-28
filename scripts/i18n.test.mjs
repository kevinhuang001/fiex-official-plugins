import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";

const read = (name) => fs.readFileSync(new URL(name, import.meta.url), "utf8");
const compiled = ts.transpileModule(read("../sdk/ui/src/i18n.ts"), {
  compilerOptions: {
    module: ts.ModuleKind.ES2022,
    target: ts.ScriptTarget.ES2022,
  },
}).outputText;
const { createI18n } = await import(
  `data:text/javascript;base64,${Buffer.from(compiled).toString("base64")}`
);
const en = JSON.parse(read("../support/i18n/en.json"));
const traditional = JSON.parse(read("../support/i18n/zh-TW.json"));

test("every official source falls back to simplified Chinese and switches live", () => {
  const runtime = createI18n({ en, "zh-TW": traditional });
  let changes = 0;
  const unsubscribe = runtime.subscribe(() => changes++);
  for (const [locale, messages] of [
    ["zh-CN", null],
    ["en", en],
    ["zh-TW", traditional],
  ]) {
    runtime.setLocale(locale);
    for (const source of Object.keys(en)) {
      assert.equal(runtime.t(source), messages?.[source] ?? source);
    }
  }
  assert.equal(changes, 2);
  unsubscribe();
  runtime.setLocale("zh-CN");
  assert.equal(changes, 2);
});

test("interpolation preserves filenames, paths and braces across locale switches", () => {
  const runtime = createI18n({ en, "zh-TW": traditional });
  const resource = "中文/{1}.txt";
  for (const locale of ["en", "zh-TW", "zh-CN"]) {
    runtime.setLocale(locale);
    assert.ok(runtime.t("已创建 {0}", resource).includes(resource));
    assert.equal(runtime.t("unknown message"), "unknown message");
  }
});

test("panel runtimes do not leak language into each other", () => {
  const a = createI18n({ en, "zh-TW": traditional }, "en");
  const b = createI18n({ en, "zh-TW": traditional }, "zh-TW");
  const before = b.t("取消");
  a.setLocale("zh-CN");
  assert.equal(b.t("取消"), before);
});
