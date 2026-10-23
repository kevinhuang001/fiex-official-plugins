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

Permissions are limited to `filesystem.read`, `filesystem.write` and `network`. Git requests `github.com`; users can add their own Gitea/GitLab hosts in permission management. `localhost` and loopback addresses are always allowed.

The **remote-integration** GitHub Actions job starts actual WebDAV and Samba servers and tests the released binaries' connection validation, directory listings, pagination, unsafe path rejection, and WebDAV file reads. Separate **core-integration** invokes the same optional packages through Fiex's real process host. Core CI does not run these protocol fixtures.

## Command execution

Plugin processes cannot spawn programs. A plugin that needs Git, ffmpeg or
another executable requests it in `permissions.commands` and calls the host's
`host.exec` proxy. The host checks the user's per-command grants, strips the
environment, blocks shells/paths, and enforces a timeout and output cap. Users
can add or remove allowed commands under **Settings → Plugins → Permissions**.

## Layout

## UI builds

Git, Archive and Checksum ship React/Vite panels under `official/<slug>/ui`.
The built single-file HTML is committed as `official/<slug>/ui/dist/index.html`, so
release packaging and CI need no Node.js. After changing UI source run
`npm ci && npm run build` in that directory and commit `ui/dist`.

