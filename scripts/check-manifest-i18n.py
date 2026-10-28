#!/usr/bin/env python3
"""Validate manifest catalogs, duplicate keys and placeholder contracts."""
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FIELDS = {"name", "description", "title", "label", "placeholder", "help", "hint", "tooltip"}
LOCALES = ("en", "zh-TW")


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def read_json(path):
    try:
        return json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=unique_object)
    except ValueError as error:
        raise ValueError(f"{path}: {error}") from error


def slots(value):
    return sorted(re.findall(r"\{([\w]*)(?::[^}]*)?\}", value))


def visible_strings(value):
    if isinstance(value, list):
        for item in value:
            yield from visible_strings(item)
    elif isinstance(value, dict):
        for key, item in value.items():
            if key in {"locales", "author", "publisher"}:
                continue
            if isinstance(item, str) and key in FIELDS:
                yield item
            else:
                yield from visible_strings(item)


def check():
    translation_map = read_json(ROOT / "scripts/manifest-translations.json")
    errors = []
    manifests = sorted((ROOT / "official").glob("*/manifest.json"))
    for path in manifests:
        manifest = read_json(path)
        catalogs = manifest.get("locales", {})
        sources = set(visible_strings(manifest))
        for value in sources:
            # Product names, formats, example hosts/paths and numeric defaults
            # are locale-neutral. Every Chinese source requires both catalogs.
            if not re.search(r"[\u3400-\u9fff]", value):
                continue
            expected = translation_map.get(value)
            if not expected:
                errors.append(f"{path}: unknown display string: {value}")
                continue
            for locale in LOCALES:
                actual = catalogs.get(locale, {}).get(value)
                if actual != expected[locale] or slots(value) != slots(actual or ""):
                    errors.append(f"{path}: missing/stale translation or invalid placeholders ({locale}): {value}")
        for locale, catalog in catalogs.items():
            if locale not in LOCALES:
                errors.append(f"{path}: unsupported locale: {locale}")
            for source in catalog:
                if source not in sources:
                    errors.append(f"{path}: unused translation: {source}")
    # Parsing with the same loader detects duplicates in every resource file.
    for locale in LOCALES:
        read_json(ROOT / f"support/i18n/{locale}.json")
    if errors:
        raise ValueError("\n".join(errors))
    print(f"Manifest i18n: {len(manifests)} plugins provide en and zh-TW translations.")


if __name__ == "__main__":
    try:
        check()
    except ValueError as error:
        raise SystemExit(str(error)) from error
