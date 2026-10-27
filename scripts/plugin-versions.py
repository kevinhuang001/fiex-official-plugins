#!/usr/bin/env python3
"""Only changed official plugins get a new version.

The release UI and release PRs use one rule: every path under
`official/<slug>/` makes that plugin "changed", and only changed plugins may
bump their manifest version. Unchanged plugins must stay byte-for-byte at the
version in the base ref.
"""
import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OFFICIAL = ROOT / "official"


def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=ROOT, text=True).strip()


def manifests() -> list[str]:
    return sorted(path.parent.name for path in OFFICIAL.glob("*/manifest.json"))


def changed_slugs(base: str) -> set[str]:
    names = git("diff", "--name-only", f"{base}...HEAD", "--", "official").splitlines()
    slugs = set()
    for name in names:
        parts = Path(name).parts
        if len(parts) >= 3 and parts[0] == "official":
            slugs.add(parts[1])
    return slugs


def version_at(ref: str, slug: str) -> str | None:
    try:
        raw = git("show", f"{ref}:official/{slug}/manifest.json")
    except subprocess.CalledProcessError:
        return None
    return json.loads(raw)["version"]


def changes(base: str) -> tuple[set[str], dict[str, tuple[str | None, str]]]:
    touched = changed_slugs(base)
    versions = {}
    for slug in manifests():
        head = json.loads((OFFICIAL / slug / "manifest.json").read_text())["version"]
        versions[slug] = (version_at(base, slug), head)
    return touched, versions


def bump(value: str, part: str) -> str:
    major, minor, patch = (int(piece) for piece in value.split(".")[:3])
    if part == "major":
        return f"{major + 1}.0.0"
    if part == "minor":
        return f"{major}.{minor + 1}.0"
    return f"{major}.{minor}.{patch + 1}"


def write_version(slug: str, value: str) -> None:
    folder = OFFICIAL / slug
    manifest_path = folder / "manifest.json"
    manifest = json.loads(manifest_path.read_text())
    manifest["version"] = value
    manifest_path.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")

    cargo = folder / "Cargo.toml"
    if cargo.exists():
        cargo.write_text(
            re.sub(r'(?m)^version\s*=\s*"[^"]+"', f'version = "{value}"', cargo.read_text(), count=1)
        )
    lock = folder / "Cargo.lock"
    if lock.exists():
        text = lock.read_text()
        text = re.sub(
            rf'(?m)(\[\[package\]\]\nname = "fiex-plugin-{re.escape(slug)}"\nversion = ")[^"]+(")',
            rf"\g<1>{value}\g<2>",
            text,
        )
        lock.write_text(text)
    root_lock = ROOT / "Cargo.lock"
    if root_lock.exists():
        text = root_lock.read_text()
        text = re.sub(
            rf'(?m)(\[\[package\]\]\nname = "fiex-plugin-{re.escape(slug)}"\nversion = ")[^"]+(")',
            rf"\g<1>{value}\g<2>",
            text,
        )
        root_lock.write_text(text)
    package = folder / "ui" / "package.json"
    if package.exists():
        data = json.loads(package.read_text())
        data["version"] = value
        package.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n")
    package_lock = folder / "ui" / "package-lock.json"
    if package_lock.exists():
        data = json.loads(package_lock.read_text())
        data["version"] = value
        if "" in data.get("packages", {}):
            data["packages"][""]["version"] = value
        package_lock.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=["check", "list", "bump"])
    parser.add_argument("--base", required=True, help="base ref/tag, e.g. origin/main or plugins-v0.4.0")
    parser.add_argument("--part", choices=["major", "minor", "patch"], default="patch")
    args = parser.parse_args()

    touched, versions = changes(args.base)
    if args.command == "list":
        for slug in sorted(touched):
            before, after = versions[slug]
            print(f"{slug}: {before or '(new)'} -> {after}")
        return

    errors = []
    for slug, (before, after) in versions.items():
        if slug in touched:
            if before is not None and after == before:
                errors.append(f"{slug} changed but version stayed {after}; run bump")
            elif before is not None and tuple(map(int, after.split("."))) < tuple(map(int, before.split("."))):
                errors.append(f"{slug} version went backwards: {before} -> {after}")
        elif before is not None and after != before:
            errors.append(f"{slug} did not change but version moved {before} -> {after}")

    if args.command == "check":
        if errors:
            print("\n".join(f"- {error}" for error in errors), file=sys.stderr)
            raise SystemExit(1)
        print(f"version check passed ({len(touched)} changed plugin(s))")
        return

    for slug in sorted(touched):
        before, after = versions[slug]
        # A new plugin starts at its declared version; bump existing plugins only.
        target = after if before is None else bump(before, args.part)
        write_version(slug, target)
        print(f"{slug}: {before or '(new)'} -> {target}")


if __name__ == "__main__":
    main()
