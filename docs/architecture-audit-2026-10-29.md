# Official plugin protocol-6 audit

Reviewed against Core PR #21 and SDK PR #9; implementation is in plugin PR #19.
The review includes source/manifest boundaries and the existing CI fixtures. It
is not a claim that every plugin workflow was exercised against real accounts.

| Plugin | Review and changes | Verification boundary |
| --- | --- | --- |
| Checksum | Ref-only bounded reader; require read before Info invocation | Unit/installed-host fixtures; 10 GiB full hash pending |
| Archive | Ref-only IO; read-gated content, capability-gated menus, preserved symlink kind, existing-directory reuse, depth limit | Unit/installed-host archive fixtures; all storage pairs pending |
| Data Preview | Ref-only content requests; read requirement on Preview/Info | Format fixtures and installed-host preview; huge files remain limited by manifest |
| Office Preview | Ref-only content requests; read requirement on Preview/Info | Format fixtures and installed-host preview |
| Google Drive | Independent Google capabilities in discovery/allowedOps; no binary read/write for native Docs; resumable file creation and overwrite; ETag/revision checks | OAuth/network/upload fixture expanded; real account/shared-drive testing pending |
| SMB | Authenticated granted-access discovery, parent delete rights, controlled TCP | Real SMB fixture and process tests; live ACL changes need real-server coverage |
| WebDAV | Current-user privilege discovery, bind/unbind permissions, controlled HTTP | Real WebDAV fixture; vendor-specific ACL behavior pending |
| OpenList | Account and directory permissions intersect; no optimistic write on a read-only directory | Connection fixture; production account variants pending |
| Git | Feature code uses typed refs/process services; VFS implementation owns node IDs | Git VFS/installed-host fixture; mixed-source Git content is semantic support, not provider identity gating |
| Everything | Search uses its declared service, not arbitrary host filesystem reads | Windows-specific process behavior; other-platform availability is manifest-defined |
| GPG | Files and process access remain scoped to declared services | Installed-host GPG fixture; real keyring/user-interaction variants pending |
| SSH | PluginView owns terminal/session UI; controlled TCP, no Core SSH business route | Encrypted SSH regression and installed-host view fixture; live servers/auth variants pending |
| Email | Connection-owned PluginView and declared transport; no generic VFS claim | Process/UI checks; real IMAP/SMTP account workflows pending |
| Vista | Theme-only package, no storage privileges; protocol change now bumps package version | Theme/manifest checks |
| Win11 | Theme-only package, no storage privileges; protocol change now bumps package version | Theme/manifest checks |

Core changes are essential to this migration: every Feature discovery path must
include operation capabilities, unknown-length output must be spooled before a
size-requiring provider starts, and returned node IDs must be bound after mkdir
and commit. Updating manifests alone would either hide every provider or leave
nested cloud writes broken.

The SDK rejects truncated read streams, so hashes and archive parsers cannot
silently treat premature EOF as a valid complete file. Core and official plugins
pin that same SDK revision. Release notes now describe protocol 6, and only
changed plugin packages are versioned relative to the last published tag.

Remaining release blockers belong to the complete architecture plan: source
switches still exist in Core mutation/search paths, platform raw-device access
is incomplete, promised-file drag is absent, and the OS/device/10 GiB acceptance
matrix is not complete. CI success is necessary but does not close those gaps.

Google capability reference: https://developers.google.com/workspace/drive/api/reference/rest/v3/files
