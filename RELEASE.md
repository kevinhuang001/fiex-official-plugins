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
