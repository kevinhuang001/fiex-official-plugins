# SSH PluginView

`ConnectionProvider::open` returns the packaged terminal HTML and an opaque, one-use session token. Credentials use secret connection fields and stay in the plugin process; private keys are supplied as content, never host filesystem paths. The UI uses `FiexUiClient` and `ssh.start`, `ssh.exchange`, `ssh.close`; xterm.js ships inside the plugin package.

`sessions.rs` owns bounded input/output buffers, resizing, completion/error state and idle cleanup. `transport.rs` handles SSH authentication, SDK-backed host-key trust and PTY transport over the SDK task-owned Tokio TCP adapter. No Core SSH IDs, terminal commands or host-terminal services are needed. Closing the view stops its session; abandoned views expire after five minutes.

CI builds the standalone UI and exercises the actual binary against an encrypted SSH fixture on Linux, macOS and Windows, including banners, stderr, authentication rejection, PTY rejection and connection failure.

SSH does not read `~/.ssh`, use OS agents or open Unix sockets. Private keys and optional passphrases arrive through generic secret fields. First-use host-key fingerprints persist through typed SDK Storage with compare-and-set; changed keys fail verification.
