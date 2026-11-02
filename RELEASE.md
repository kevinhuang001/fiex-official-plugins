# Fiex plugins 0.5.11

- Migrate all changed official providers to the shared typed SDK contracts and align VFS, ContextMenu, Search, Info, Preview, Decoration, Hook, Toolbar, and Connection calls with the unified Core provider pipeline.
- Publish new patch versions only for the 13 plugins changed by this migration.

- Google Drive native Docs, Sheets and Slides now declare Write and are updated by uploading the exported DOCX/XLSX/PPTX with a Google Workspace conversion target. The editor ID, permissions and location stay unchanged while the full document content is replaced.
- WebDAV, OpenList and Drive file reads use long-lived HTTP/aria2 timeouts so large downloads do not fail at the old 90–120 second request limit.
- Providers now use SDK 0.3.3 and the Core Rust aria2 engine; no external aria2c binary is required.

# Fiex plugins 0.5.9

- WebDAV, OpenList and Google Drive expose optional direct HTTP sources so Core can use aria2 for large downloads. Providers still own authorization; Core falls back to the normal stream when aria2 is unavailable.
- Google Drive native editors export by type (DOCX/XLSX/PPTX/PNG/JSON) and report the exported size, so Office preview and open work instead of reporting an incomplete export.

# Fiex plugins 0.5.8

- Google Drive native Docs, Sheets, Slides and Drawings are exported as PDF for preview and download. The editor document stays read-only in Fiex and is edited in Google's web apps.

# Fiex plugins 0.5.5 — protocol 6

Official storage plugins implement `VfsProvider` over `vfs.*`; Feature plugins use
ResourceRef-only `Files` over `host.fs.*`. Deploy with the protocol-6 Core and Rust SDK
0.3.0. Old protocol-5 packages cannot be activated against this contract.

Checksum, Archive, Data Preview and Office Preview declare their required read
operation. Archive menus use content capabilities and file kind, never source or
mount identity; extraction reuses existing directories and rejects a file where
a directory is needed. Compression bounds both entry count and traversal depth.

Core integration covers installed plugin processes and network fixtures. Google Drive now reports independent server capabilities and creates new files
through resumable upload; existing writes retain revision/ETag protection. Physical
USB hotplug and external drag/drop still require supported-platform validation.

# Previous plugin release notes

SSH owns its terminal UI and sessions in a self-contained PluginView. Every native plugin uses the shared typed Providers API with protocol 6 and SDK 0.3.x. CI uses sccache on all Rust jobs and tests actual process protocols, encrypted SSH and installed plugins against Core.

# Fiex plugins 0.5.0

- Plugin manifests now use schema 2, scoped event providers, and per-plugin versioning. Unchanged plugins keep their existing versions.
- Archive, Office preview and Data preview ship package-local loading pages. Core keeps the loading page visible until the preview calls `preview.ready` after its first data render.
- Archive can extract through the `beforeOpen` hook, grant sibling output folders and poll Core-owned extraction tasks.
- New GPG keychain plugin: encrypt, decrypt, sign and verify through the system GnuPG, plus a keychain panel with public/secret keys, fingerprints, import, export and delete.
- New Email plugin: IMAP/SMTP connection, Gmail-style plugin view, list/read/send, with no local mailbox copy.
- New SSH and Google Drive plugins.
- Checksum no longer ships a legacy panel UI; checksum stays available through Info and the context menu.
- Only plugins whose `official/<slug>/` files changed receive a version bump.

# Versioning

Only plugins whose `official/<slug>/` files changed get a new version. Do not
bump every official plugin for one release.

Use **Actions → Prepare plugin release** (workflow_dispatch): enter the previous
`plugins-v*` tag and `patch`/`minor`/`major`. It opens a PR that bumps only the
changed plugins. The PR check enforces the same rule.

```bash
# Bump only changed plugins, relative to the previous release tag:
python scripts/plugin-versions.py bump --base plugins-v0.4.0 --part patch

# Review, then let CI enforce the rule:
python scripts/plugin-versions.py check --base plugins-v0.4.0
```

The script updates `manifest.json`, `Cargo.toml`, `Cargo.lock`,
`ui/package.json` and `ui/package-lock.json` for changed plugins only. The
release catalog and release page read those manifest versions.

# Fiex plugins 0.4.0

- New data-preview plugin (replaces sqlite-preview): paged, sortable tables for SQLite, CSV/TSV, Excel, Parquet, Arrow IPC and Avro.
- Excel previews use data-preview (priority 110) over office-preview (100); DOCX/PPTX still use office-preview.
- data-preview and office-preview entry files are split into open/dispatch and sanitize/render modules.

# Fiex plugins 0.3.0

- Archive registers a `beforeOpen` hook: double-clicking an archive extracts it into the archive's folder (conflict-safe folder names) instead of opening the system app. Password-protected archives hand off to the preview.
- Archive gains an archive preview panel: list entries before extracting, enter a password when needed, and extract to the current directory from the preview.

# Fiex plugins 0.2.10

- Office preview now parses DOCX/XLSX/PPTX with the `office_oxide` Office library instead of hand-written XML walks: headings, lists, tables, sheet boundaries and slide boundaries are rendered as structured HTML.
- The plugin sanitizes the generated HTML (no scripts, handlers or javascript: URLs) before rendering it.

# Fiex plugins 0.2.9

- New SQLite viewer plugin: tables, columns, indexes, foreign keys and table data with paging and column sorting, read-only.
- Plugin previewers now render inside the Quick Look preview sheet, not in a floating panel.

# Fiex plugins 0.2.8

- New OpenList plugin: native API client (login, directory listing, file reads), not WebDAV. The server address is a connection (so Core approves exactly that host); username, password and timeout live in the plugin settings.
- New Office preview plugin: read-only Word, Excel and PowerPoint previews (text-level; charts, animations and legacy formats are out of scope).
- Previewers are pluggable: a plugin declares `contributions.preview` with extensions and a priority, so it can override a built-in previewer such as PDF.
- Archive's ignore lists use the new list setting type: rows with add/remove instead of a comma-separated field.
- New GPG plugin: right-click 加密 / 解密 / 签名 / 验证签名, plus a toolbar key manager. It drives the system GnuPG; private-key passphrases stay in pinentry.
- New Vista Aero and Windows 11 theme packages. A theme is a declarative token set with no executable, iframe or permissions.
- Official panels follow the host theme tokens and size themselves to their content.
- Archive’s advanced-compression panel opens from its submenu entry again (requires Fiex 0.4.9).

# Fiex plugins 0.2.7

- Archive and Checksum wait for the bridge context before rendering their empty-selection hints, removing the “请选择…” flash when the panel opens.

# Fiex plugins 0.2.6

- Archive’s advanced-compression panel declares a compact 520×320 content size.
- Plugin panel size is controlled by the plugin manifest instead of Core’s old fixed dimensions.

# Fiex plugins 0.2.5

- Git status badges now also color the filename: modified orange, added green, deleted red, renamed blue, untracked gray.
- Decoration badges sit after the filename.

# Fiex plugins 0.2.4

- Archive uses separate first-level context entries: “压缩”, “解压缩”, and “高级”; “压缩到…” lives inside “高级”.
- Compression defaults to a unique archive name in the current folder; extraction defaults to the current folder.
- Compression formats are shown only for non-archive selections, extraction only for archive files. Long nested format menus flip upward to stay inside the window.
- Official panels no longer render title/close headers; Core provides the icon, title and close button.

# Fiex plugins 0.2.3

- Archive is context-menu only: compress, extract and advanced compression no