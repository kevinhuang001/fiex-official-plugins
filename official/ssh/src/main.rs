//! SSH over the Host-brokered TCP stream. The plugin never opens a socket
//! directly: `ctx.tcp()` resolves and connects through Core's network broker,
//! then the SSH handshake and channel run over that stream.
use base64::{engine::general_purpose::STANDARD, Engine};
use fiex_plugin_sdk::tcp::TcpStream;
use fiex_plugin_sdk::{self as sdk, RpcError};
use russh::{
    client::{self, Handler},
    keys::PublicKeyOrCertificate,
    keys::{agent::client::AgentClient, load_secret_key, PrivateKeyWithHashAlg},
};
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    path::Path,
    sync::Arc,
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};

struct ServerKey {
    host: String,
    port: u16,
}

impl Handler for ServerKey {
    type Error = russh::Error;

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

/// Adapt the Host-brokered blocking stream to the AsyncRead/AsyncWrite stream
/// required by russh. Two small pump threads keep all socket I/O inside Core.
fn bridge(tcp: TcpStream) -> Result<DuplexStream, RpcError> {
    let (client, server) = tokio::io::duplex(64 * 1024);
    let mut reader = tcp.clone();
    let mut writer = tcp;
    let (to_sdk, mut to_sdk_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(16);
    let (from_sdk, mut from_sdk_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(16);
    std::thread::spawn(move || {
        let mut buffer = [0u8; 16 * 1024];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(size) => {
                    if from_sdk.blocking_send(buffer[..size].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    std::thread::spawn(move || {
        while let Some(data) = to_sdk_rx.blocking_recv() {
            if writer.write_all(&data).is_err() {
                break;
            }
        }
    });
    let (mut server_reader, mut server_writer) = tokio::io::split(server);
    tokio::spawn(async move {
        while let Some(data) = from_sdk_rx.recv().await {
            if server_writer.write_all(&data).await.is_err() {
                break;
            }
        }
    });
    tokio::spawn(async move {
        let mut buffer = [0u8; 16 * 1024];
        loop {
            match server_reader.read(&mut buffer).await {
                Ok(0) | Err(_) => break,
                Ok(size) => {
                    if to_sdk.send(buffer[..size].to_vec()).await.is_err() {
                        break;
                    }
                }
            }
        }
    });
    Ok(client)
}

fn open_terminal(ctx: sdk::RequestContext) -> Result<Value, RpcError> {
    let p = ctx.params();
    let session_id = text(p, "sessionId").to_string();
    if session_id.is_empty() {
        return Err(RpcError::internal("missing terminal session"));
    }
    let connection = p
        .get("connection")
        .ok_or_else(|| RpcError::internal("missing SSH connection"))?;
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

    // The TCP socket is created by Core's network broker. The plugin only gets
    // a Read + Write stream and cannot bypass the host allowlist.
    let tcp = ctx
        .tcp()
        .connect(&host, port)
        .map_err(|error| RpcError::internal(format!("SSH TCP 连接失败：{error}")))?;

    ctx.spawn_internal_task("SSH 终端", move |task| {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|error| RpcError::internal(error.to_string()))?;
        runtime.block_on(async move {
            let stream = bridge(tcp)?;
            let key_handler = ServerKey {
                host: host.clone(),
                port,
            };
            let mut handle = client::connect_stream(
                Arc::new(client::Config::default()),
                stream,
                key_handler,
            )
            .await
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

            let channel = handle
                .channel_open_session()
                .await
                .map_err(|error| RpcError::internal(error.to_string()))?;
            channel
                .request_pty(true, "xterm-256color", cols, rows, 0, 0, &[])
                .await
                .map_err(|error| RpcError::internal(format!("SSH PTY 请求失败：{error}")))?;
            channel
                .request_shell(true)
                .await
                .map_err(|error| RpcError::internal(format!("SSH shell 启动失败：{error}")))?;

            let (mut read_half, write_half) = channel.split();
            let mut reader = read_half.make_reader();
            let mut writer = write_half.make_writer();
            let mut input = Vec::<u8>::new();
            let mut last_size = (cols, rows);
            let mut buffer = [0u8; 16 * 1024];
            let mut tick = tokio::time::interval(Duration::from_millis(8));

            loop {
                tokio::select! {
                    result = reader.read(&mut buffer) => {
                        match result {
                            Ok(0) => break,
                            Ok(size) => {
                                let data = STANDARD.encode(&buffer[..size]);
                                if task.invoke("host.terminal.output", json!({
                                    "sessionId": session_id,
                                    "dataBase64": data
                                })).is_err() {
                                    break;
                                }
                            }
                            Err(error) => {
                                let _ = task.invoke("host.terminal.end", json!({
                                    "sessionId": session_id,
                                    "error": error.to_string()
                                }));
                                return Ok(Value::Null);
                            }
                        }
                    }
                    _ = tick.tick() => {
                        let closed = task.invoke("host.terminal.closed", json!({"sessionId": session_id}))
                            .ok()
                            .and_then(|value| value.get("closed").and_then(Value::as_bool))
                            .unwrap_or(true);
                        if closed { break; }

                        if let Ok(value) = task.invoke("host.terminal.input", json!({
                            "sessionId": session_id,
                            "maxBytes": 8192
                        })) {
                            if value.get("eof").and_then(Value::as_bool).unwrap_or(false) {
                                break;
                            }
                            if let Some(data) = value.get("dataBase64").and_then(Value::as_str) {
                                if let Ok(bytes) = STANDARD.decode(data) {
                                    input.extend(bytes);
                                }
                            }
                        }
                        if !input.is_empty() {
                            if let Err(error) = writer.write_all(&input).await {
                                let _ = task.invoke("host.terminal.end", json!({
                                    "sessionId": session_id,
                                    "error": error.to_string()
                                }));
                                return Ok(Value::Null);
                            }
                            input.clear();
                        }

                        if let Ok(value) = task.invoke("host.terminal.size", json!({"sessionId": session_id})) {
                            let current_cols = value.get("cols").and_then(Value::as_u64).unwrap_or(80) as u32;
                            let current_rows = value.get("rows").and_then(Value::as_u64).unwrap_or(24) as u32;
                            if (current_cols, current_rows) != last_size {
                                let _ = write_half.window_change(current_cols, current_rows, 0, 0).await;
                                last_size = (current_cols, current_rows);
                            }
                        }
                    }
                }
            }
            let _ = writer.shutdown().await;
            let _ = task.invoke("host.terminal.end", json!({"sessionId": session_id}));
            Ok(Value::Null)
        })
    })?;

    Ok(json!({ "started": true }))
}

fn run(ctx: sdk::RequestContext) -> Result<Value, RpcError> {
    let p = ctx.params();
    match ctx.method() {
        "connection.validate" => {
            let host = p
                .pointer("/connection/host")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            Ok(json!({ "valid": !host.is_empty() }))
        }
        "terminal.open" => open_terminal(ctx),
        _ => Err(RpcError::method_not_found(ctx.method())),
    }
}

fn main() {
    if let Err(error) = sdk::serve("org.fiex.ssh", run) {
        eprintln!("SSH provider failed: {error}");
        std::process::exit(1);
    }
}
