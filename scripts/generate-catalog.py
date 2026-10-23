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
SLUGS = ["git", "webdav", "smb", "openlist", "office-preview", "archive", "checksum", "everything", "gpg", "vista", "win11"]
DESCRIPTIONS = {
    "git": "源代码管理、提交历史、远程仓库与只读历史浏览。",
    "webdav": "通过独立进程连接 WebDAV 服务器，目录按需读取。",
    "openlist": "通过 OpenList 原生 API 连接服务器，账号密码在插件设置中配置。",
    "office-preview": "Word、Excel、PowerPoint 的只读预览（文本级）。",
    "smb": "通过独立进程连接 SMB / NAS 共享。",
    "archive": "压缩与解压 ZIP、7z、TAR 等格式。",
    "checksum": "计算 SHA-256、SHA-512、SHA-1、MD5 与 CRC32。",
    "everything": "借用 Everything 的 Windows 本地索引进行秒级文件名搜索。",
}

def main() -> None:
    out = {"schemaVersion": 1, "plugins": []}
    for slug in SLUGS:
        manifest_path = Path("official") / slug / "manifest.json"
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        permissions = []
        requested = manifest.get("permissions", {})
        folders = requested.get("folders", {})
        if folders.get("read", {}).get("enabled"):
            permissions.append("folders.read")
        for action in ("create", "delete", "rename"):
            if folders.get(action):
                permissions.append("folders." + action)
        for action in ("read", "write"):
            if requested.get("files", {}).get(action):
                permissions.append("files." + action)
        if requested.get("commands", {}).get("enabled"):
            permissions.append("commands")
        network = manifest.get("permissions", {}).get("network", {})
        if network.get("enabled"):
            permissions.append("network")
        permissions.sort()
        commands = manifest.get("permissions", {}).get("commands", {}).get("allow", [])
        out["plugins"].append(
            {
                "id": manifest["id"],
                "name": manifest["name"],
                "version": manifest["version"],
                "description": manifest.get("description") or DESCRIPTIONS.get(slug, ""),
                "icon": manifest.get("icon", "puzzle"),
                "author": manifest.get("author"),
                "permissions": permissions,
                "networkDomains": manifest.get("permissions", {}).get("network", {}).get("domains", []),
                "commands": commands,
                "assets": {
                    platform: f"{slug}-{platform}.fiexplugin"
                    for platform in PLATFORMS
                    if platform in manifest.get("runtime", {}).get("entrypoints", {}) or manifest.get("contributions", {}).get("themes")
                },
            }
        )
    target = Path("dist/plugins/fiex-catalog.json")
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(json.dumps(out, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print("Created", target)

if __name__ == "__main__":
    main()
