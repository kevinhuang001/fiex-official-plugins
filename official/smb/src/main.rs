//! SMB protocol implementation. All socket operations are brokered by Core.
mod sync;
mod transport;
use fiex_plugin_sdk::{self as sdk, RpcError};
use serde::Deserialize;
use serde_json::{json, Value};
use smb::{GetLen, ReadAt};
use std::{
    collections::HashMap,
    str::FromStr,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    time::Duration,
};

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredConnection {
    host: String,
    share: String,
    #[serde(default)]
    base: String,
    #[serde(default)]
    username: String,
    #[serde(default)]
    password: String,
    port: Option<u16>,
}
/// A real remote file kept open for the lifetime of one `fs.openRead` stream.
/// Reusing the connection, authenticated session, tree and file handle avoids
/// reconnecting and re-authenticating for every read chunk.
struct OpenStream {
    connection: smb::Connection,
    session: smb::Session,
    tree: smb::Tree,
    file: smb::File,
    total: u64,
}

enum StreamCommand {
    Read {
        offset: u64,
        max: usize,
        reply: mpsc::Sender<Result<Value, RpcError>>,
    },
    Close {
        reply: mpsc::Sender<Result<Value, RpcError>>,
    },
}

/// The stream is owned by a background task so its TCP connection, SMB
/// session and remote file handle stay authorized after the opening request
/// returns and are reclaimed when the stream is closed or cancelled.
struct StreamHandle {
    sender: mpsc::Sender<StreamCommand>,
}

impl Drop for OpenStream {
    fn drop(&mut self) {
        let _ = self.file.close();
        let _ = self.tree.disconnect();
        let _ = self.session.logoff();
        let _ = self.connection.close();
    }
}

fn connect(
    tcp: &sdk::TcpClient,
    cfg: &StoredConnection,
) -> Result<(smb::Connection, smb::Session, smb::Tree), String> {
    let port = cfg.port.unwrap_or(445);
    let transport =
        transport::HostTransport::open(tcp, &cfg.host, port).map_err(|error| error.to_string())?;
    let connection = smb::Connection::from_transport(
        Box::new(transport),
        &cfg.host,
        smb::Guid::generate(),
        smb::ConnectionConfig::default(),
    )
    .map_err(|error| error.to_string())?;
    let identity = sspi::AuthIdentity {
        username: sspi::Username::parse(&cfg.username).map_err(|error| error.to_string())?,
        password: sspi::Secret::from(cfg.password.clone()),
    };
    let session = connection
        .authenticate(identity)
        .map_err(|error| error.to_string())?;
    let unc = smb::UncPath::from_str(&format!("\\\\{}\\{}", cfg.host, cfg.share))
        .map_err(|error| error.to_string())?;
    let tree = session
        .tree_connect(&unc)
        .map_err(|error| error.to_string())?;
    Ok((connection, session, tree))
}

fn open_stream(
    tcp: &sdk::TcpClient,
    cfg: &StoredConnection,
    path: &str,
) -> Result<OpenStream, RpcError> {
    let (connection, session, tree) = connect(tcp, cfg).map_err(RpcError::internal)?;
    let path = smb_path(cfg, path);
    let file = tree
        .create(
            &path,
            &smb::FileCreateArgs::make_open_existing(
                smb::FileAccessMask::new().with_generic_read(true),
            ),
        )
        .map_err(|error| RpcError::internal(error.to_string()))?
        .unwrap_file();
    let total = file
        .get_len()
        .map_err(|error| RpcError::internal(error.to_string()))?;
    Ok(OpenStream {
        connection,
        session,
        tree,
        file,
        total,
    })
}

fn read_open_stream(stream: &OpenStream, offset: u64, max: usize) -> Result<Value, RpcError> {
    let mut bytes = vec![0; max];
    let read = stream
        .file
        .read_at(&mut bytes, offset)
        .map_err(|error| RpcError::internal(error.to_string()))?;
    bytes.truncate(read);
    use base64::Engine;
    Ok(json!({
        "base64":base64::engine::general_purpose::STANDARD.encode(bytes),
        "length":read,
        "bytesRead":read,
        "eof":offset.saturating_add(read as u64)>=stream.total
    }))
}
fn safe_relative(path: &str) -> Result<(), RpcError> {
    if path.starts_with('/')
        || path.contains(['\\', '\0'])
        || path.split('/').any(|p| p == "." || p == "..")
    {
        return Err(RpcError::invalid_params("invalid remote path"));
    }
    Ok(())
}
fn smb_path(cfg: &StoredConnection, path: &str) -> String {
    [cfg.base.trim_matches('/'), path.trim_matches('/')]
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("\\")
}
fn remote(
    tcp: sdk::TcpClient,
    cfg: StoredConnection,
    path: String,
    method: String,
    params: Value,
) -> Result<Value, RpcError> {
    let logical = path.clone();
    let run = || -> Result<Value, String> {
        let port = cfg.port.unwrap_or(445);
        let transport =
            transport::HostTransport::open(&tcp, &cfg.host, port).map_err(|e| e.to_string())?;
        let connection = smb::Connection::from_transport(
            Box::new(transport),
            &cfg.host,
            smb::Guid::generate(),
            smb::ConnectionConfig::default(),
        )
        .map_err(|e| e.to_string())?;
        let identity = sspi::AuthIdentity {
            username: sspi::Username::parse(&cfg.username).map_err(|e| e.to_string())?,
            password: sspi::Secret::from(cfg.password.clone()),
        };
        let session = connection
            .authenticate(identity)
            .map_err(|e| e.to_string())?;
        let unc = smb::UncPath::from_str(&format!("\\\\{}\\{}", cfg.host, cfg.share))
            .map_err(|e| e.to_string())?;
        let tree = session.tree_connect(&unc).map_err(|e| e.to_string())?;
        let path = smb_path(&cfg, &path);
        let result = if matches!(method.as_str(), "connection.validate" | "fs.list") {
            let dir = tree
                .create(
                    &path,
                    &smb::FileCreateArgs::make_open_existing(
                        smb::DirAccessMask::new()
                            .with_list_directory(true)
                            .with_synchronize(true)
                            .into(),
                    ),
                )
                .map_err(|e| e.to_string())?
                .unwrap_dir();
            let mut entries = Vec::new();
            for item in dir
                .query::<smb::FileFullDirectoryInformation>("*")
                .map_err(|e| e.to_string())?
            {
                let item = item.map_err(|e| e.to_string())?;
                let name = item.file_name.to_string();
                if name == "." || name == ".." {
                    continue;
                }
                if entries.len() >= 100000 {
                    return Err("remote directory too large".into());
                }
                let node = if logical.is_empty() {
                    name.clone()
                } else {
                    format!("{}/{}", logical, name)
                };
                entries.push(json!({"nodeId":node,"name":name,"kind":if item.file_attributes.directory(){"directory"}else{"file"},"size":item.end_of_file,"modified":0,"revision":format!("{:?}:{:?}:{}",item.last_write_time,item.change_time,item.end_of_file)}));
            }
            let _ = dir.close();
            entries.sort_by_key(|v| v["name"].as_str().unwrap_or("").to_owned());
            if method == "connection.validate" {
                json!({"valid":true})
            } else {
                let start = params
                    .get("cursor")
                    .and_then(Value::as_str)
                    .unwrap_or("0")
                    .parse::<usize>()
                    .map_err(|e| e.to_string())?;
                if start > entries.len() {
                    return Err("invalid pagination cursor".into());
                }
                let limit = params
                    .get("limit")
                    .and_then(Value::as_u64)
                    .unwrap_or(200)
                    .clamp(1, 500) as usize;
                let end = start.saturating_add(limit).min(entries.len());
                json!({"entries":entries[start..end],"nextCursor":if end<entries.len(){Some(end.to_string())}else{None},"snapshotToken":null,"identityStability":"path"})
            }
        } else {
            let file = tree
                .create(
                    &path,
                    &smb::FileCreateArgs::make_open_existing(
                        smb::FileAccessMask::new().with_generic_read(true),
                    ),
                )
                .map_err(|e| e.to_string())?
                .unwrap_file();
            let total = file.get_len().map_err(|e| e.to_string())?;
            let max = params
                .get("maxBytes")
                .and_then(Value::as_u64)
                .unwrap_or(262144)
                .clamp(1, 1048576) as usize;
            let offset = params.get("offset").and_then(Value::as_u64).unwrap_or(0);
            let mut bytes = vec![0; max];
            let read = file
                .read_at(&mut bytes, offset)
                .map_err(|e| e.to_string())?;
            bytes.truncate(read);
            use base64::Engine;
            let result = json!({"base64":base64::engine::general_purpose::STANDARD.encode(bytes),"length":read,"bytesRead":read,"eof":offset.saturating_add(read as u64)>=total});
            let _ = file.close();
            result
        };
        let _ = tree.disconnect();
        let _ = session.logoff();
        let _ = connection.close();
        Ok(result)
    };
    run().map_err(RpcError::internal)
}
fn main() {
    let streams: Mutex<HashMap<String, Arc<StreamHandle>>> = Mutex::new(HashMap::new());
    let sequence = AtomicU64::new(0);
    let result = sdk::serve("org.fiex.smb", move |ctx| {
        let method = ctx.method();
        if matches!(method, "fs.stat" | "fs.writeFile") {
            return sync::dispatch(&ctx);
        }
        let p = ctx.params();
        if !matches!(
            method,
            "connection.validate"
                | "fs.list"
                | "fs.readSmall"
                | "fs.openRead"
                | "fs.readStream"
                | "fs.closeRead"
        ) {
            return Err(RpcError::method_not_found(method));
        }
        let stream_id = p.get("streamId").and_then(Value::as_str).unwrap_or("");
        if method == "fs.closeRead" {
            let stream = streams
                .lock()
                .map_err(|_| RpcError::internal("stream registry unavailable"))?
                .remove(stream_id);
            if let Some(stream) = stream {
                let (reply, response) = mpsc::channel();
                let _ = stream.sender.send(StreamCommand::Close { reply });
                let _ = response.recv_timeout(Duration::from_secs(30));
            }
            return Ok(Value::Null);
        }
        if method == "fs.readStream" {
            let stream = streams
                .lock()
                .map_err(|_| RpcError::internal("stream registry unavailable"))?
                .get(stream_id)
                .cloned()
                .ok_or_else(|| RpcError::invalid_params("unknown read stream"))?;
            let max = p
                .get("maxBytes")
                .and_then(Value::as_u64)
                .unwrap_or(262144)
                .clamp(1, 1048576) as usize;
            let offset = p.get("offset").and_then(Value::as_u64).unwrap_or(0);
            let (reply, response) = mpsc::channel();
            stream
                .sender
                .send(StreamCommand::Read { offset, max, reply })
                .map_err(|_| RpcError::internal("SMB stream is closed"))?;
            return response
                .recv_timeout(Duration::from_secs(120))
                .map_err(|_| RpcError::internal("SMB stream stopped responding"))?;
        }
        let cfg: StoredConnection = serde_json::from_value(
            p.get("connection")
                .cloned()
                .ok_or_else(|| RpcError::invalid_params("missing connection"))?,
        )
        .map_err(|e| RpcError::invalid_params(e.to_string()))?;
        let path = p
            .get("directoryId")
            .or_else(|| p.get("nodeId"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        safe_relative(&path)?;
        safe_relative(&cfg.base)?;
        if method == "fs.openRead" {
            let (init_sender, init_receiver) = mpsc::channel();
            let (sender, receiver) = mpsc::channel();
            let task = ctx.spawn_internal_task("SMB 读取流", move |task| {
                let tcp = task.tcp();
                let stream = match open_stream(&tcp, &cfg, &path) {
                    Ok(stream) => stream,
                    Err(error) => {
                        let _ = init_sender.send(Err(error.clone()));
                        return Err(error);
                    }
                };
                let _ = init_sender.send(Ok(stream.total));
                loop {
                    if task.is_cancelled() {
                        break;
                    }
                    match receiver.recv_timeout(Duration::from_millis(200)) {
                        Ok(StreamCommand::Read { offset, max, reply }) => {
                            let _ = reply.send(read_open_stream(&stream, offset, max));
                        }
                        Ok(StreamCommand::Close { reply }) => {
                            let _ = reply.send(Ok(Value::Null));
                            break;
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
                Ok(Value::Null)
            })?;
            let total = match init_receiver.recv_timeout(Duration::from_secs(30)) {
                Ok(Ok(total)) => total,
                Ok(Err(error)) => {
                    task.cancel();
                    return Err(error);
                }
                Err(_) => {
                    task.cancel();
                    return Err(RpcError::internal("SMB stream did not open"));
                }
            };
            let mut streams = streams
                .lock()
                .map_err(|_| RpcError::internal("stream registry unavailable"))?;
            if streams.len() >= 8 {
                task.cancel();
                return Err(RpcError::internal("too many read streams"));
            }
            let id = format!("smb-read-{}", sequence.fetch_add(1, Ordering::Relaxed) + 1);
            streams.insert(id.clone(), Arc::new(StreamHandle { sender }));
            return Ok(json!({"size": total, "streamId": id}));
        }
        remote(ctx.tcp(), cfg, path, method.to_owned(), p.clone())
    });
    if let Err(error) = result {
        eprintln!("SMB plugin IPC failed: {error}");
        std::process::exit(1);
    }
}
