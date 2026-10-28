#!/usr/bin/env python3
"""Generate the release catalog consumed by Fiex Settings -> Plugins.

The release contract is intentionally small:
  tag:    plugins-v<semver>
  assets: fiex-catalog.json
          <slug>-<platform>.fiexplugin   (one per plugin/platform)
"""
import json
from pathlib import Path

PLATFORMS = [
    "linux-x86_64",
    "macos-aarch64",
    "macos-x86_64",
    "windows-x86_64",
]
def catalog_entry(slug: str, manifest: dict) -> dict:
    if manifest.get("schemaVersion") != 2:
        raise ValueError(f"{slug}: expected manifest schema 2")
    requested = manifest.get("permissions", {})
    permissions = []
    for family, actions in [("folders", ("read", "create", "delete", "rename")),
                            ("files", ("read", "write", "create", "delete", "rename"))]:
        for action in actions:
            if requested.get(family, {}).get(action, {}).get("enabled", False):
                permissions.append(f"{family}.{action}")
    for name in ("commands", "hooks", "metadata"):
        if requested.get(name, {}).get("enabled", False):
            permissions.append(name)
    network = requested.get("network", {})
    if network.get("enabled") or network.get("all") or network.get("domains"):
        permissions.append("network")
    extensions = manifest.get("extensions", {})
    return {
        "id": manifest["id"], "name": manifest["name"], "version": manifest["version"],
        "description": manifest.get("description", ""), "icon": manifest.get("icon", "puzzle"),
        "author": manifest.get("author"), "permissions": sorted(permissions),
        "networkDomains": network.get("domains", []),
        "commands": requested.get("commands", {}).get("allow", []),
        "extensions": extensions,
        "assets": {platform: f"{slug}-{platform}.fiexplugin" for platform in PLATFORMS
                   if platform in manifest.get("runtime", {}).get("entrypoints", {}) or extensions.get("themes")},
    }

def main() -> None:
    out = {"schemaVersion": 1, "plugins": []}
    for manifest_path in sorted(Path("official").glob("*/manifest.json")):
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        out["plugins"].append(catalog_entry(manifest_path.parent.name, manifest))
    target = Path("dist/plugins/fiex-catalog.json")
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(json.dumps(out, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print("Created", target)
    # One description file for every plugin; the store reads its text as the
    # plugin introduction, so copy can change without repackaging binaries.
    descriptions = {
        "schemaVersion": 1,
        "plugins": [
            {
                "id": plugin["id"],
                "name": plugin["name"],
                "description": plugin["description"],
            }
            for plugin in out["plugins"]
        ],
    }
    intro = target.parent / "plugin-descriptions.json"
    intro.write_text(json.dumps(descriptions, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print("Created", intro)

if __name__ == "__main__":
    main()
