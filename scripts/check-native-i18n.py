#!/usr/bin/env python3
"""Inspect native display messages without rewriting protocols or resource data."""
from pathlib import Path
import json
import re
import sys
from tree_sitter import Language, Parser
import tree_sitter_rust

ROOT = Path(__file__).resolve().parents[1]
PARSER = Parser(Language(tree_sitter_rust.language()))

def nodes(node):
    yield node
    for child in node.children:
        yield from nodes(child)

def messages():
    for path in sorted((ROOT / "official").glob("*/src/**/*.rs")):
        code = path.read_bytes()
        for node in nodes(PARSER.parse(code).root_node):
            if node.type != "string_literal":
                continue
            ancestors = []
            parent = node.parent
            while parent:
                ancestors.append(parent)
                parent = parent.parent
            test_function = any(parent.type == "function_item" and parent.prev_named_sibling is not None and b"[test]" in code[parent.prev_named_sibling.start_byte:parent.prev_named_sibling.end_byte] for parent in ancestors)
            if test_function or "/tests/" in str(path) or any(
                parent.type == "mod_item" and re.search(rb"mod\s+tests\b", code[parent.start_byte:parent.start_byte + 100])
                for parent in ancestors
            ):
                continue
            try:
                source = json.loads(code[node.start_byte:node.end_byte])
            except (ValueError, UnicodeError):
                continue
            translated = bool(re.search(rb"i18n::t\s*\(\s*$", code[max(0, node.start_byte - 80):node.start_byte])) or any(
                parent.type == "call_expression" and re.search(rb"i18n::t\s*\($", code[parent.start_byte:node.start_byte])
                for parent in ancestors
            )
            # Archive cancellation is an internal comparison token, never a display label.
            sentinel = any(parent.type == "const_item" and b"const CANCELLED:" in code[parent.start_byte:parent.end_byte] for parent in ancestors)
            if translated or (re.search(r"[\u3400-\u9fff]", source) and not sentinel):
                yield path, node.start_point[0] + 1, source, translated

if __name__ == "__main__":
    if "--keys" in sys.argv:
        print(json.dumps(sorted({source for _, _, source, translated in messages() if translated})))
        raise SystemExit(0)
    en = json.loads((ROOT / "support/i18n/en.json").read_text())
    errors = []
    count = 0
    for path, line, source, translated in messages():
        count += 1
        if not translated:
            errors.append(f"{path.relative_to(ROOT)}:{line}: untranslated native display message: {source}")
        elif source not in en:
            errors.append(f"{path.relative_to(ROOT)}:{line}: missing native translation: {source}")
    if errors:
        raise SystemExit("\n".join(errors))
    print(f"Native i18n: {count} call sites verified.")
