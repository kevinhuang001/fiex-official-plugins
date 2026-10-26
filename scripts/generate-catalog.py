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
SLUGS = ["ssh", "git", "webdav", "gdrive", "smb", "openlist", "office-preview", "data-preview", "archive", "checksum", "everything", "gpg", "email", "vista", "win11"]
DESCRIPTIONS = {
    "git": "源代码管理、提交历史、远程仓库与只读历史浏览。",
    "webdav": "通过独立进程连接 WebDAV 服务器，目录按需读取。",
    "openlist": "通过 OpenList 原生 API 连接服务器，账号密码在插件设置中配置。",
    "office-preview": "Word、Excel、PowerPoint 的只读预览（文本级）。",
    "data-preview": "表格数据预览：SQLite、CSV、Excel、Parquet、Arrow、Avro，支持分页排序。",
    "smb": "通过独立进程连接 SMB / NAS 共享。",
    "gdrive": "Google Drive 按需下载、离线副本与双向同步。",
    "gpg": "使用本机 GnuPG 加密、解密、签名、验签及管理公钥。",
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
        files = requested.get("files", {})
        # Every operation is an object with an explicit `enabled` flag. A
        # present-but-disabled object must not be reported as a grant, or the
        # store would ask the user for permissions the plugin never declared.
        def enabled(value):
            return isinstance(value, dict) and value.get("enabled") is True

        if enabled(requested.get("metadata")):
            permissions.append("metadata")
        if enabled(folders.get("read")):
            permissions.append("folders.read")
        for action in ("create", "delete", "rename"):
            if enabled(folders.get(action)):
                permissions.append("folders." + action)
        for action in ("read", "write", "create", "delete", "rename"):
            if enabled(files.get(action)):
                permissions.append("files." + action)
        network = requested.get("network", {})
        if network.get("enabled"):
            permissions.append("network")
        if requested.get("commands", {}).get("enabled"):
            permissions.append("commands")
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
                "extensions": manifest.get("extensions", {}),
                "assets": {
                    platform: f"{slug}-{platform}.fiexplugin"
                    for platform in PLATFORMS
                    if platform in manifest.get("runtime", {}).get("entrypoints", {}) or manifest.get("extensions", {}).get("themes")
                },
            }
        )
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
