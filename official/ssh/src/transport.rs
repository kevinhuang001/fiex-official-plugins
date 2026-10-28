//! SSH over the Host-brokered TCP stream. The plugin never opens a socket
//! directly: SDK TCP resolves and connects through the host network broker,
//! then the SSH handshake and channel run over that stream.
use crate::sessions::{self, Terminal};
use fiex_plugin_sdk::{self as sdk, RpcError};
#[cfg(unix)]
use russh::keys::agent::client::AgentClient;
use russh::{
    client::{self, Handler},
    keys::PublicKeyOrCertificate,
    keys::{load_secret_key, PrivateKeyWithHashAlg},
};
use serde_json::{json, Value};
use std::{path::Path, sync::Arc, time::Duration};
use tokio::io::AsyncWriteExt;

// Unblock any pending SDK read before Tokio tears down its blocking workers.
struct StreamGuard(sdk::TcpStream);
impl Drop for StreamGuard {
    fn drop(&mut self) {
        let _ = self.0.shutdown(std::net::Shutdown::Both);
    }
}

struct ServerKey {
    host: String,
    port: u16,
    terminal: Terminal,
}

impl Handler for ServerKey {
    type Error = russh::Error;

    async fn auth_banner(
        &mut self,
        banner: &str,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        let _ = self.terminal.output(banner.as_bytes());
        Ok(())
    }
    async fn check_server_key(
        &mut self,
        key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let public = key.public_key();
        if russh::keys::check_known_hosts(&self.host, self.port, &public).unwrap_or(false) {
            return Ok(true);
        }
        // First use: learn the host key (equivalent to ssh accept-new). A
        // changed key for a host already in known_hosts remains rejected.
        let known = russh::keys::known_hosts::known_host_keys(&self.host, self.port)
            .map(|entries| !entries.is_empty())
            .unwrap_or(false);
        if known {
            return Ok(false);
        }
        let _ = russh::keys::known_hosts::learn_known_hosts(&self.host, self.port, &public);
        Ok(true)
    }
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

fn expanded(value: &str) -> String {
    if let Some(rest) = value.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return Path::new(&home).join(rest).to_string_lossy().into_owned();
        }
    }
    value.to_string()
}

async fn request_accepted(
    channel: &mut russh::Channel<client::Msg>,
    request: &str,
) -> Result<(), RpcError> {
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            match channel.wait().await {
                Some(russh::ChannelMsg::Success) => return Ok(()),
                Some(russh::ChannelMsg::Failure) => {
                    return Err(RpcError::internal(format!("SSH server rejected {request}")))
                }
                Some(russh::ChannelMsg::Close | russh::ChannelMsg::Eof) | None => {
                    return Err(RpcError::internal(format!(
                        "SSH channel closed during {request}"
                    )))
                }
                _ => {}
            }
        }
    })
    .await
    .map_err(|_| RpcError::internal(format!("SSH {request} timed out")))?
}

pub(crate) fn open(ctx: sdk::RequestContext) -> Result<Value, RpcError> {
    let p = ctx.params();
    let session_id = text(p, "sessionId").to_string();
    if session_id.is_empty() {
        return Err(RpcError::internal("missing terminal session"));
    }
    let (connection, terminal) = sessions::start(&session_id)?;
    let connection = &connection;
    let host = text(connection, "host").to_string();
    if host.is_empty() {
        return Err(RpcError::internal("SSH 主机为空"));
    }
    let port = connection
        .get("port")
        .and_then(Value::as_u64)
        .unwrap_or(22)
        .clamp(1, 65535) as u16;
    let cols = p
        .get("cols")
        .and_then(Value::as_u64)
        .unwrap_or(80)
        .clamp(20, 65535) as u32;
    let rows = p
        .get("rows")
        .and_then(Value::as_u64)
        .unwrap_or(24)
        .clamp(4, 65535) as u32;
    let username = text(connection, "username");
    let username = if username.is_empty() {
        "root"
    } else {
        username
    }
    .to_string();
    let identity = expanded(text(connection, "identityFile"));
    let password = text(connection, "password").to_string();

    let terminal_type = match text(connection, "terminalType") {
        "" => "xterm-256color",
        value => value,
    }
    .to_owned();
    ctx.spawn_internal_task("SSH 终端", move |task| {
        let task = Arc::new(task);
        let stream_terminal = terminal.clone();
        let result = (|| {
        let terminal = stream_terminal;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|error| RpcError::internal(error.to_string()))?;
        runtime.block_on(async {
            let stream = task
                .tcp()
                .connect_async(host.clone(), port)
                .await
                .map_err(|error| RpcError::internal(format!("SSH TCP 连接失败：{error}")))?;
            let _transport = StreamGuard(stream.get_ref().clone());
            let key_handler = ServerKey {
                host: host.clone(),
                port,
                terminal: terminal.clone(),
            };
            let mut handle = tokio::time::timeout(Duration::from_secs(30),
                client::connect_stream(Arc::new(client::Config::default()), stream, key_handler))
                .await.map_err(|_| RpcError::internal("SSH handshake timed out"))?
                .map_err(|error| RpcError::internal(format!("SSH 握手失败：{error}")))?;

            let mut authenticated = false;
            if !identity.is_empty() {
                if let Ok(key) = load_secret_key(&identity, None) {
                    authenticated = handle
                        .authenticate_publickey(
                            &username,
                            PrivateKeyWithHashAlg::new(Arc::new(key), None),
                        )
                        .await
                        .map(|result| result.success())
                        .unwrap_or(false);
                }
            }
            #[cfg(unix)]
            if !authenticated {
                if let Ok(mut agent) = AgentClient::connect_env().await {
                    if let Ok(identities) = agent.request_identities().await {
                        for identity in identities {
                            let key = identity.public_key().into_owned();
                            if handle
                                .authenticate_publickey_with(&username, key, None, &mut agent)
                                .await
                                .map(|result| result.success())
                                .unwrap_or(false)
                            {
                                authenticated = true;
                                break;
                            }
                        }
                    }
                }
            }
            if !authenticated && !password.is_empty() {
                authenticated = handle
                    .authenticate_password(&username, &password)
                    .await
                    .map(|result| result.success())
                    .unwrap_or(false);
            }
            if !authenticated {
                return Err(RpcError::internal(
                    "SSH 认证失败：请检查私钥、ssh-agent 或密码",
                ));
            }

            let mut channel = handle
                .channel_open_session()
                .await
                .map_err(|error| RpcError::internal(error.to_string()))?;
            channel
                .request_pty(true, &terminal_type, cols, rows, 0, 0, &[])
                .await
                .map_err(|error| RpcError::internal(format!("SSH PTY 请求失败：{error}")))?;
            request_accepted(&mut channel, "PTY").await?;
            channel
                .request_shell(true)
                .await
                .map_err(|error| RpcError::internal(format!("SSH shell 启动失败：{error}")))?;

            request_accepted(&mut channel, "shell").await?;
            terminal.ready()?;
            let (mut read_half, write_half) = channel.split();
            let (output_tx, mut output_rx) =
                tokio::sync::mpsc::channel::<Result<Vec<u8>, String>>(16);
            tokio::spawn(async move {
                while let Some(message) = read_half.wait().await {
                    let result = match message {
                        russh::ChannelMsg::Data { data } | russh::ChannelMsg::ExtendedData { data, .. } => Ok(data.to_vec()),
                        russh::ChannelMsg::Eof | russh::ChannelMsg::Close => break,
                        russh::ChannelMsg::ExitSignal { error_message, signal_name, .. } => Err(format!("SSH shell exited ({signal_name:?}): {error_message}")),
                        russh::ChannelMsg::ExitStatus { exit_status } if exit_status != 0 => Err(format!("SSH shell exited with status {exit_status}")),
                        _ => continue,
                    };
                    if output_tx.send(result).await.is_err() { return; }
                }
                let _ = output_tx.send(Ok(Vec::new())).await;
            });
            let mut writer = write_half.make_writer();
            let mut input = Vec::<u8>::new();
            let mut last_size = (cols, rows);
            let mut tick = tokio::time::interval(Duration::from_millis(8));


            loop {
                tokio::select! {
                    incoming = output_rx.recv() => {
                        match incoming {
                            Some(Ok(bytes)) if bytes.is_empty() => break,
                            Some(Ok(bytes)) => {
                                if terminal.output(&bytes).is_err() {
                                    break;
                                }
                            }
                            Some(Err(error)) => {
                                return Err(RpcError::internal(error));
                            }
                            None => break,
                        }
                    }
                    _ = tick.tick() => {
                        if terminal.closed().unwrap_or(true) {
                            break;
                        }

                        if let Ok(pending) = terminal.input(8192) {
                            if pending.eof {
                                break;
                            }
                            input.extend(pending.data);
                        }
                        if !input.is_empty() {
                            if let Err(error) = writer.write_all(&input).await {
                                return Err(RpcError::internal(error.to_string()));
                            }
                            input.clear();
                        }

                        if let Ok(size) = terminal.size() {
                            let current = (size.cols as u32, size.rows as u32);
                            if current != last_size {
                                let _ = write_half.window_change(current.0, current.1, 0, 0).await;
                                last_size = current;
                            }
                        }
                    }
                }
            }
            let _ = writer.shutdown().await;
            Ok(Value::Null)
        })
        })();
        let _ = terminal.end(result.as_ref().err().map(|error: &RpcError| error.message.as_str()));
        result
    })?;

    Ok(json!({ "started": true }))
}
