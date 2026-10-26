# Email

A Fiex connection provider for standard IMAP/SMTP mailboxes. It opens a
Gmail-style mailbox as a **pluginView** connection; it does not create a VFS,
mount files, or save messages locally.

## Connection

Add a connection from the Fiex sidebar and choose **邮箱**:

- 邮箱地址: full address, e.g. `you@example.com`
- IMAP/SMTP 授权码: app password (QQ Mail calls it 授权码)
- IMAP 服务器: defaults to `imap.qq.com`
- SMTP 服务器: defaults to `smtp.qq.com`

The password is stored by Core in the system credential store under the
connection scope; the panel only receives the account, not the password.

## Network

All TCP connections go through the Fiex SDK (`ctx.tcp().connect`). The plugin
wraps that stream in rustls and speaks IMAP/SMTP itself; Core remains the only
component that authorizes and opens the socket. Declared domains:
`imap.qq.com`, `smtp.qq.com`.

## Live check

```bash
FIEX_EMAIL=you@example.com \
FIEX_EMAIL_PASSWORD='app-password' \
cargo run --example live_check
```

The check logs in over IMAP, lists and reads a message, then sends a test
message over SMTP. Credentials are read from the environment and are never
stored in the repository.
