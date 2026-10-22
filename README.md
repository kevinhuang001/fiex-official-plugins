# Fiex Official Plugins

Independent optional native process plugins for [Fiex](https://github.com/kevinhuang001/fiex): Git, WebDAV, SMB, Archive and Checksum.

**No plugins ship inside the Fiex application.** All plugins, including these official examples, require the user to download, install and approve the declared permissions. Publishing here grants no special trust.

The SDK is a pinned Git submodule under `sdk/`. Run `git submodule update --init sdk` after cloning.

```sh
cargo test --manifest-path official/git/Cargo.toml
cargo test --manifest-path official/checksum/Cargo.toml
```

Release packages are published by plugin CI under tags `plugins-v*`, separately from Fiex Core releases. In Fiex Settings → Plugins, use the GitHub repository `kevinhuang001/fiex-official-plugins`.
