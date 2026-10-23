# Fiex plugins 0.2.3

- Archive is context-menu only: compress, extract and advanced compression no longer appear as a toolbar tool.
- All official plugin versions are 0.2.3; package and UI versions match.
- Official plugin panels no longer render their own title/close header; the Core iframe supplies icon, title and close.

# Fiex plugins 0.2.2

- Every released package is signed with the publisher Ed25519 key. Core verifies the manifest signature and every file digest.
- Git exposes remote configuration, pull and push in its React panel; init, commit, pull and push report task name, detail, step and progress to Status Center.
- Archive reports compression tasks to Status Center.
- Checksum remains a read-only view and does not create activity entries.
- Plugin UI source uses the updated `fiex.tasks` API.

# Fiex plugins 0.2.1

- Every official plugin embeds its manifest with `include_str!` and returns it on `plugin.initialize`; Core verifies the embedded copy against the installed package.
- The plugin package still carries `manifest.json` because Core must validate permissions and locate the executable before spawning anything; the embedded copy prevents a shipped binary from being paired with a different manifest later.
- All plugins report 0.2.1. Git, Archive and Checksum keep their React single-file panels.

# Fiex plugins 0.2.0

- All six plugins report version 0.2.0.
- Git links libgit2 through `git2`; it never executes a local Git binary or uses `host.exec`. Remote pull is fast-forward only.
- Git ships a React panel built to one static HTML file: status, staging, commit, history and worktree return.
- Archive adds RAR extraction and creation via the pure-Rust `rars` codec, and ships a React advanced-compression panel.
- Checksum ships a React multi-file checksum panel.
- Git, Archive and Checksum declare inline `panel` entries; Core has no feature-specific UI or commands.
- Plugin processes run with Arapuca `allow_exec = false`. A plugin that requests `permissions.commands` still goes through the host proxy after explicit per-command approval; none of the official plugins need it now.
- UI source lives under `official/<slug>/ui`; the built single-file HTML is committed under `ui/dist` so packaging needs no Node.js.
