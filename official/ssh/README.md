# SSH PluginView

`ConnectionProvider::open` returns the packaged terminal HTML and an opaque, one-use session token. Credentials stay in the plugin process. The UI uses `FiexUiClient` and `ssh.start`, `ssh.exchange`, `ssh.close`; xterm.js ships inside the plugin package.

`sessions.rs` owns bounded input/output buffers, resizing, completion/error state and idle cleanup. `transport.rs` handles SSH authentication, host keys and PTY transport over the SDK task-owned Tokio TCP adapter. No Core SSH IDs, terminal commands or host-terminal services are needed. Closing the view stops its session; abandoned views expire after 60 seconds.

CI builds the standalone UI and exercises the actual binary against an encrypted SSH fixture on Linux, macOS and Windows, including banners, stderr, authentication rejection, PTY rejection and connection failure.
