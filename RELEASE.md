# Fiex plugins 0.1.1

- Git plugin 0.1.1 runs every Git command through the host `host.exec` proxy.
- Plugin processes are sandboxed with `allow_exec = false` and cannot spawn programs directly.
- Manifest `permissions.commands` requests executable names; users approve and can edit the list in Settings.
- Git history, status, staging, commits, remotes and VFS browsing continue to work through the proxy.
- Everything search, WebDAV, SMB, Archive and Checksum remain at 0.1.0.

# Fiex plugins 0.1.0

- Six optional plugins: Git, WebDAV, SMB, Archive, Checksum and Windows Everything search.
- Manifest schema 1 with permissions and contributions separated.
- Connections register remote VFS; toolbar/context-menu entries carry inline panels.
- Protocol 2.0 plugin processes.
