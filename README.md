# Fiex Official Plugins

> License: GPL-3.0-or-later. These plugins are optional; nothing is bundled with Fiex.

Independent optional native process plugins for [Fiex](https://github.com/kevinhuang001/fiex): Git, WebDAV, SMB, Archive, Checksum and the Windows Everything search provider.

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
- `slug`: `git | webdav | smb | archive | checksum | everything`.
- `platform`: `linux-x86_64 | macos-aarch64 | macos-x86_64 | windows-x86_64`.

Fiex Settings → Plugins reads the catalog from the latest release; there is no manual repository input.

Permissions are reviewed individually at activation. Installation leaves the plugin disabled. File capabilities are `folders.read`, `folders.create`, `folders.delete`, `folders.rename`, `files.read` and `files.write`. `network` has one exact-host allowlist shared by HTTP and TCP, including localhost and IP literals. Valid ports on an approved host are available to both transports. Revoking a grant disables the plugin.

Native plugin processes have no user-file access, direct network sockets or subprocess execution. Archive and Checksum use the high-level file SDK; WebDAV uses the HTTP SDK; SMB supplies its protocol over the SDK TCP stream. Git logic remains in this repository and invokes the user's PATH Git through the generic Host command runner. Everything similarly requires the optional ES command in PATH instead of contacting native window-message IPC directly.
The **remote-integration** GitHub Actions job starts actual WebDAV and Samba servers and tests the released binaries' connection validation, directory listings, pagination, unsafe path rejection, and WebDAV file reads. Separate **core-integration** invokes the same optional packages through Fiex's real process host. Core CI does not run these protocol fixtures.

## Command execution

Plugin processes cannot spawn programs. Approved external commands are resolved through the host's PATH and run as the current user outside the plugin sandbox. Plugins supply argument arrays. These commands can access files outside SDK scopes, use the network and start other programs; the activation dialog explains this general risk without plugin-specific warnings. Timeouts and output limits apply, but command approval is an explicit trust decision.

## Layout

## UI builds

Git, Archive and Checksum ship React/Vite panels under `official/<slug>/ui`.
The built single-file HTML is committed as `official/<slug>/ui/dist/index.html`, so
release packaging and CI need no Node.js. After changing UI source run
`npm ci && npm run build` in that directory and commit `ui/dist`.


## Integration ownership

Business-specific host integration fixtures live in `integration/examples`, not
in the Core repository. Check out the matching Core revision under `core/`, then
run `cargo run --manifest-path integration/Cargo.toml --example plugin_git_e2e`
and `plugin_optional_e2e` with isolated `FIEX_PLUGIN_DIR` and `XDG_DATA_HOME`.
Core's own CI uses the standalone SDK mock process.
