#!/usr/bin/env python3
"""Verify all user-facing manifest strings have both translations."""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
translation_map = json.loads((ROOT / "scripts/manifest-translations.json").read_text(encoding="utf-8"))
FIELDS = {"name", "description", "title", "label", "placeholder", "help", "hint", "tooltip"}
def visible_strings(value):
    if isinstance(value, list):
        for item in value:
            yield from visible_strings(item)
    elif isinstance(value, dict):
        for key, item in value.items():
            if key == "locales":
                continue
            if isinstance(item, str) and key in FIELDS:
                yield item
            else:
                yield from visible_strings(item)

errors = []
for path in sorted((ROOT / "official").glob("*/manifest.json")):
    manifest = json.loads(path.read_text(encoding="utf-8"))
    catalogs = manifest.get("locales", {})
    for value in set(visible_strings(manifest)):
        if not any("\u3400" <= char <= "\u9fff" for char in value):
            continue
        expected = translation_map.get(value)
        if not expected:
            errors.append(f"{path}: unknown display string: {value}")
            continue
        for locale in ("en", "zh-TW"):
            actual = catalogs.get(locale, {}).get(value)
            if actual != expected[locale]:
                errors.append(f"{path}: missing/stale {locale}: {value}")
    for locale, catalog in catalogs.items():
        if locale not in ("en", "zh-TW"):
            errors.append(f"{path}: unsupported locale {locale}")
        for source in catalog:
            if source not in set(visible_strings(manifest)):
                errors.append(f"{path}: unused translation: {source}")
if errors:
    raise SystemExit("\n".join(errors))
print("All official plugin manifests provide en and zh-TW translations.")
