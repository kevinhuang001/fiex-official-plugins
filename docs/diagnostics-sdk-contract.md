# Official plugin diagnostics

Baseline: official PR #22 (`efc2e3a3c8dea34a7f80cd60b101d3d1fa3c31b8`),
SDK PR #13 (`1fdee12f0720dec5180ee73699dd256c293250e3`). Companion Core PR #28.

All 13 native official plugins opt into `Providers::new().diagnostics()`:
Archive, Checksum, Data Preview, Email, Everything, Google Drive, Git, GPG,
Office Preview, OpenList, SMB, SSH and WebDAV. The two themes contain no native
runtime, and do not acquire an artificial diagnostics process.

The SDK observes typed provider dispatch, custom RPC and inherited background
work. Successful requests are debug diagnostics; unsupported/cancelled RPC codes
remain debug outcomes; genuine failures are warn diagnostics. Task failures use
error logs, while task cancellation stays quiet. Diagnostic failure never replaces
the operation result. Progress/chunk payloads, credentials, resource names and
raw errors are not copied into automatic lifecycle records.

Request failures return once to Core/calling UI; the SDK does not also request a
modal. Background hook failures that consume an open action explicitly use
`TaskContext::report_error`. Archive does not report normal cancellation or a
successful inline fallback as a failure. Its standard compression/extraction tasks
remain in Core's task centre and gain SDK lifecycle logs.

All official panels use the shared UI SDK client. Panel ready and SSH connection
transitions use `client.logs.write`; uncaught render errors/rejections use
`client.errors.report`. These route through native SDK diagnostics endpoints and
then authenticated Host calls. Panels have no direct Tauri/log storage calls and
cannot choose a plugin identity. Caught business/RPC errors retain their existing
local UI instead of generating a duplicate modal. Fatal transport teardown uses
credential-free stderr because the authenticated request channel is unavailable.

Versions are incremented only for the 13 changed native packages. Core and this
repository pin the same companion SDK commit; no protocol version change is
needed. Fixtures service SDK diagnostic Host requests separately from business
RPC responses and assert that operation parameters do not leak into logs.

Local validation: SDK Rust/UI tests, official UI builds and i18n checks,
representative native executables and process-level diagnostics. Full native
plugin builds and Windows/macOS behavior remain covered by repository CI; live
SMB/WebDAV/SSH/Google credentials are not assumed available locally.
