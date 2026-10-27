# Fiex Official Plugins

> License: GPL-3.0-or-later. These plugins are optional; nothing is bundled with Fiex.

Independent optional native process plugins for [Fiex](https://github.com/kevinhuang001/fiex): Git, WebDAV, SMB, OpenList, Office preview, data viewer, Archive, Checksum and the Windows Everything search provider.

**No plugins ship inside the Fiex application.** All plugins, including these official examples, require the user to download, install and approve the declared permissions. Publishing here grants no special trust.

The SDK is a pinned Git submodule under `sdk/`. Run `git submodule update --init sdk` after cloning.

```sh
cargo test --manifest-path official/git/Cargo.toml
cargo test --manifest-path official/checksum/Cargo.toml
cargo test --manifest-path official/everything/Cargo.toml
```

Release packages are published under tags `plugins-v*`, separately from Fiex Core releases.

Release contract:

- `fiex-catalog.json` — app-store catalog (`id`, `name`, `permissions`, `networkDomains`, per-platform assets).
- `<slug>-<platform>.fiexplugin` — one package per plugin/platform.
- `slug`: `git | webdav | gdrive | smb | openlist | office-preview | data-preview | archive | checksum | everything | gpg | vista | win11`.
- `platform`: `linux-x86_64 | macos-aarch64 | macos-x86_64 | windows-x86_64`.

Fiex Settings → Plugins reads the catalog from the latest release; there is no manual repository input.

Permissions are reviewed individually at activation. Installation leaves the plugin disabled. File capabilities are `metadata`, `folders.read/create/delete/rename` and `files.read/write/create/delete/rename`, each granted independently. `network` has one exact-host allowlist shared by HTTP and TCP, including localhost and IP literals. Valid ports on an approved host are available to both transports. Revoking a grant disables the plugin.

Native plugin processes have no user-file access, direct network sockets or subprocess execution. Archive and Checksum use the high-level file SDK; WebDAV uses the HTTP SDK; SMB supplies its protocol over the SDK TCP stream. Git logic remains in this repository and invokes the user's PATH Git through the generic Host command runner. Everything similarly requires the optional ES command in PATH instead of contacting native window-message IPC directly.
The **remote-integration** GitHub Actions job starts actual WebDAV and Samba servers and tests the released binaries' connection validation, directory listings, pagination, unsafe path rejection, and WebDAV file reads. Separate **core-integration** invokes the same optional packages through Fiex's real process host. Core CI does not run these protocol fixtures.

## Command execution

Plugin processes cannot spawn programs. Approved external commands are resolved through the host's PATH and run as the current user outside the plugin sandbox. Plugins supply argument arrays. These commands can access files outside SDK scopes, use the network and start other programs; the activation dialog explains this general risk without plugin-specific warnings. Timeouts and output limits apply, but command approval is an explicit trust decision.

## UI and theme guides

The end-to-end guides live in the Core repository:

- [Manifest extensions](https://github.com/kevinhuang001/fiex/blob/HEAD/docs/plugins/02-MANIFEST-EXTENSIONS.md)
  — extension registrations, permissions and navigation targets.
- [Runtime UI panels](https://github.com/kevinhuang001/fiex/blob/HEAD/docs/plugins/09-RUNTIME-UI-PANELS.md)
  — panel sessions, themes and the sandboxed UI bridge.

## Plugin development example

New to native plugins? [`example/README.md`](example/README.md) is a beginner
manual with a buildable plugin under `example/hello/` covering connections,
VFS navigation, Info/Preview standard events, decorations (colors/badges/row
and column styles), context menus, search, settings, hooks and toolbar
panels/commands.

## Typed provider API

Standard events (`info.resolve`, `preview.open`, `decorations.resolve`,
`contextMenu.resolve/execute`, `search.query`, `hooks.invoke`, `toolbar.invoke`,
`connection.*`) can be registered with the optional `sdk::Providers` API
instead of a hand-written `match ctx.method()`. File access is one typed
`FileSystemProvider`: annotate the impl with `#[fiex_sdk::filesystem]` and the
macro declares exactly the implemented operations and generates the `fs.*`
dispatcher. The SDK owns read/write stream IDs, offsets, `commit`/`abort` and
cleanup. Custom RPC remains in the `serve_with` fallback.

```rust
sdk::Providers::new()
    .info(MyInfo)
    .preview(MyPreview)
    .context_menu(MyMenu)
    .connection(MyConnection)
    .filesystem(MyDrive)
    .serve_with("org.example.plugin", |ctx| custom(ctx))
```

The beginner example at `example/hello` and its guides use this API.

## Info and preview contracts

Info and preview are standard events, not command/panel declarations. A plugin
registers `extensions.info` / `extensions.preview` with `match` and
optional `priority`/`maxBytes` (no per-contribution id/title). Both fixed
methods receive an `EventContext` with `targets`; `ctx.targets()` and
`ctx.metadata(ref)` expose permission-projected metadata without extra RPCs.
Core validates `{ sections: [...] }` / `PreviewView`; preview HTML is returned as
a package-confined `view.entry` instead of being listed in the manifest. See
`fiex/docs/plugins/08-UNIFIED-PREVIEW-INFO.md`.

Archive, Office preview and Data preview declare a package-local
`extensions.preview.loading` document. Core shows that document while the real
preview iframe loads in the background. The preview calls `preview.ready` after
its first data render, and Core then removes the loading document, so users never
see an empty plugin shell or a second spinner.

## UI builds

SSH, Git, Archive, Checksum and GPG ship connection/panel surfaces under `official/<slug>/`. SSH uses the Host TCP broker (`ctx.tcp()`) and `network.all` at activation; it supports private-key/password/agent auth. CI runs it against a real local sshd via `scripts/test-ssh-plugin.py`. WebDAV/SMB default to `localhost` and add the exact host when a connection is saved.
CI builds the panels once, uploads them as `plugin-panels`, and uses the same
artifact in every platform package job. Generated `ui/dist` files are not source
files. All panels consume the Host theme tokens through the UI SDK.

## Themes

Vista Aero and Windows 11 are declarative theme packages: no executable, no iframe,
no permissions. Install and enable them, then select the theme in Appearance.
`manifest.json` declares `extensions.themes: [{"path":"theme.json"}]`.
The JSON theme contains `id`, `name`, `mode` (`light` or `dark`) and a `tokens`
map. Core namespaces IDs by plugin and validates package-confined paths and token
values. Tokens define colors, radii, shadows, gradients and fonts; raw CSS and
scripts are not accepted. These themes preserve Fiex's layout.

## GPG

Install GnuPG and make `gpg` available on PATH. Activate the plugin and explicitly
approve the generic `gpg` command grant. File actions are a right-click
integration: select files, then use **GPG → 加密 / 解密 / 签名 / 验证签名**.
**公钥管理** is a toolbar tool (`toolbar` contribution) so the keyring stays
reachable without a selection. Every entry opens the same panel document with the
clicked contribution id in `context.get().panelId`; the panel renders only that
action instead of a tab switcher.

The panel supports encryption, decryption, detached signatures, verification,
public-key import and export. It uses the existing keyring/Agent; private-key
passphrases stay in pinentry, never in the panel or plugin settings. Select full
fingerprints from the key list. Existing output files are not overwritten, and
unverified recipients require an explicit toggle (`--trust-model always` is only
added when that toggle is on). Signature failure is reported as an error with the
GnuPG status reason.
Core's command runtime deadline applies; cancellation of a UI wait does not
terminate an already-running GnuPG operation.

CI's `plugin_gpg_e2e` creates an isolated temporary keyring and checks a binary
round trip, valid and tampered signatures, overwrite refusal and theme loading
through the real Core host. It never uses the runner's regular keyring.

## Integration ownership

Business-specific host integration fixtures live in `integration/examples`, not
in the Core repository. Check out the matching Core revision under `core/`, then
run `cargo run --manifest-path integration/Cargo.toml --example plugin_git_e2e`
and `plugin_optional_e2e` with isolated `FIEX_PLUGIN_DIR` and `XDG_DATA_HOME`.
Core's own CI uses the standalone SDK mock process.

## Core-owned virtual views and connection I/O

Standalone virtual trees declare `extensions.vfs`; connection factories declare `extensions.connections` and also register `extensions.vfs` when they expose files. No provider declares a URI scheme or a `capabilities` block. Core generates all `fiex://mount/<id>/` navigation URIs and enforces provider ownership and selected-resource scope.

Git calls `ctx.open_vfs_view` after resolving an immutable commit and serves `fs.list/stat/read/search` through the supplied `viewContext` and opaque `nodeId` (the blob path). It does not snapshot repositories or participate in connection sync.

All uploads use `fs.beginWrite` / `fs.writeStream` / `fs.commit`, with `fs.abort` discarding an uncommitted stream. Google Drive implements resumable chunking in its writer and finishes in the commit callback. Manifest `sync`, `writeMode`, `conflictControl` switches and duplicated category `capabilities` fields are removed.

`test-git-vfs.py`, `test-gdrive.py` and `test-connection-sync.py` exercise real provider processes against deterministic brokers. The separate Core-host and live WebDAV/SMB CI jobs retain actual sandbox/network integration coverage.

## Protocol 5 provider pattern

Official native plugins register standard events with `sdk::Providers` and the matching typed provider traits. `#[sdk::filesystem]` reports only the filesystem operations actually implemented. Custom business RPC stays in `serve_with` fallback handlers; connection-only providers use `serve`.

File arguments are `ResourceRef`s bound by Core. Use `ctx.target()` / `ctx.targets()` for selections, `ctx.files()` for controlled IO, typed Storage/Secrets services, and `ctx.terminal(session_id)` for terminal operations. SSH uses the SDK's optional Tokio TCP adapter inside its background-task authorization instead of a private stream pump.

The cross-repository tests target Fiex Core `v0.7.4`; the SDK is pinned to its tested `v0.1.0` release commit. Plugin versions are compared with the previous published catalog so no release downgrades an installed plugin.
