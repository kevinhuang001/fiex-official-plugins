//! SMB network protocol adapter. Mount registry, cache and sync owned by host.
use fiex_plugin_sdk::{self as sdk, RpcError};
use futures_util::{pin_mut, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use smb::{GetLen, ReadAt};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    str::FromStr,
};
use tokio::runtime::Builder;

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
    #[serde(default)]
    network_domains: Vec<String>,
}
struct RemoteFile {
    name: String,
    is_dir: bool,
    size: u64,
    modified: u64,
}
fn remote_join(base: &str, rel: &str) -> String {
    [base.trim_matches('/'), rel.trim_matches('/')]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}
fn trim_host(value: &str) -> String {
    value
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase()
}
fn is_loopback(host: &str) -> bool {
    let host = trim_host(host);
    host == "localhost"
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

fn safe_relative(value: &str) -> Result<(), RpcError> {
    if value.starts_with('/')
        || value
            .split('/')
            .any(|p| p == "." || p == ".." || p.contains(['\\', '\0']))
    {
        return Err(RpcError::invalid_params("invalid node ID"));
    }
    Ok(())
}
struct ReadStream {
    // Drop the file/handle before the client connection: SMB close relies on
    // the client still being alive, and Rust drops struct fields in order.
    file: smb::File,
    _client: smb::Client,
    size: u64,
}
type Streams = Rc<RefCell<HashMap<String, ReadStream>>>;

fn dispatch(
    method: &str,
    p: Value,
    rt: &tokio::runtime::Runtime,
    streams: &Streams,
    next_stream: &Cell<u64>,
) -> Result<Value, RpcError> {
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
    // Stream reads are bound to an already-opened handle and never resend
    // credentials. Only the methods that initiate a connection parse it.
    let streaming = matches!(method, "fs.readStream" | "fs.closeRead");
    let config: StoredConnection = if streaming {
        StoredConnection::default()
    } else {
        serde_json::from_value(
            p.get("connection")
                .cloned()
                .ok_or_else(|| RpcError::invalid_params("missing connection"))?,
        )
        .map_err(|e| RpcError::invalid_params(e.to_string()))?
    };
    if !streaming {
        let target = trim_host(&config.host);
        if !is_loopback(&target)
            && !config
                .network_domains
                .iter()
                .any(|domain| trim_host(domain) == target)
        {
            return Err(RpcError::invalid_params("network domain not granted"));
        }
    }
    let path = p
        .get("directoryId")
        .or_else(|| p.get("nodeId"))
        .and_then(Value::as_str)
        .unwrap_or("");
    safe_relative(path)?;
    match method {
        "connection.validate" => {
            rt.block_on(smb_list(&config, ""))
                .map_err(RpcError::internal)?;
            Ok(json!({"valid":true}))
        }
        "fs.list" => {
            let mut nodes = rt
                .block_on(smb_list(&config, path))
                .map_err(RpcError::internal)?;
            nodes.sort_by(|a, b| a.name.cmp(&b.name));
            let start = p
                .get("cursor")
                .and_then(Value::as_str)
                .unwrap_or("0")
                .parse::<usize>()
                .map_err(|_| RpcError::invalid_params("invalid cursor"))?;
            if start > nodes.len() {
                return Err(RpcError::invalid_params("cursor out of range"));
            }
            let limit = p
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(200)
                .clamp(1, 500) as usize;
            let end = (start + limit).min(nodes.len());
            let entries=nodes[start..end].iter().map(|f|json!({
                "nodeId":if path.is_empty(){f.name.clone()}else{format!("{path}/{}",f.name)},
                "name":f.name,"kind":if f.is_dir{"directory"}else{"file"},
                "size":f.size,"modified":f.modified
            })).collect::<Vec<_>>();
            Ok(
                json!({"entries":entries,"nextCursor":if end<nodes.len(){Some(end.to_string())}else{None},
                "snapshotToken":null,"identityStability":"path"}),
            )
        }
        "fs.readSmall" => {
            if path.is_empty() {
                return Err(RpcError::invalid_params("missing node ID"));
            }
            let max = p
                .get("maxBytes")
                .and_then(Value::as_u64)
                .unwrap_or(262144)
                .clamp(1, 1048576) as usize;
            let bytes = rt
                .block_on(smb_read_small(&config, path, max))
                .map_err(RpcError::internal)?;
            use base64::Engine;
            Ok(json!({
                "base64": base64::engine::general_purpose::STANDARD.encode(&bytes),
                "length": bytes.len()
            }))
        }
        "fs.openRead" => {
            if path.is_empty() {
                return Err(RpcError::invalid_params("missing node ID"));
            }
            let (client, unc) = rt
                .block_on(smb_client(&config))
                .map_err(RpcError::internal)?;
            let smb_path = smb_path(&config, path);
            let file = rt
                .block_on(async {
                    Ok::<_, String>(
                        client
                            .create_file(
                                &unc.with_path(&smb_path),
                                &smb::FileCreateArgs::make_open_existing(
                                    smb::FileAccessMask::new().with_generic_read(true),
                                ),
                            )
                            .await
                            .map_err(|e| e.to_string())?
                            .unwrap_file(),
                    )
                })
                .map_err(RpcError::internal)?;
            let size = rt
                .block_on(file.get_len())
                .map_err(|e| RpcError::internal(e.to_string()))?;
            let stream_id = format!("stream-{}", next_stream.get() + 1);
            next_stream.set(next_stream.get() + 1);
            streams.borrow_mut().insert(
                stream_id.clone(),
                ReadStream {
                    _client: client,
                    file,
                    size,
                },
            );
            Ok(json!({"streamId": stream_id, "size": size}))
        }
        "fs.readStream" => {
            let stream_id = p
                .get("streamId")
                .and_then(Value::as_str)
                .ok_or_else(|| RpcError::invalid_params("missing streamId"))?;
            let offset = p
                .get("offset")
                .and_then(Value::as_u64)
                .ok_or_else(|| RpcError::invalid_params("missing offset"))?;
            let max = p
                .get("maxBytes")
                .and_then(Value::as_u64)
                .ok_or_else(|| RpcError::invalid_params("missing maxBytes"))?;
            if max == 0 || max > 256 * 1024 {
                return Err(RpcError::invalid_params("invalid chunk size"));
            }
            let streams = streams.borrow();
            let stream = streams
                .get(stream_id)
                .ok_or_else(|| RpcError::invalid_params("unknown read stream"))?;
            let mut bytes = vec![0u8; max as usize];
            let read = rt
                .block_on(stream.file.read_at(&mut bytes, offset))
                .map_err(|e| RpcError::internal(e.to_string()))?;
            bytes.truncate(read);
            let eof = stream.size > 0 && offset.saturating_add(read as u64) >= stream.size;
            use base64::Engine;
            Ok(
                json!({"base64":base64::engine::general_purpose::STANDARD.encode(&bytes),
                "bytesRead":bytes.len(),"eof":eof}),
            )
        }
        "fs.closeRead" => {
            let stream_id = p
                .get("streamId")
                .and_then(Value::as_str)
                .ok_or_else(|| RpcError::invalid_params("missing streamId"))?;
            // Dropping the stream releases the SMB handle and its client
            // connection. The client's Drop spawns a Tokio task, so it must
            // happen inside the runtime context; the control channel itself
            // stays responsive because no network round-trip is awaited.
            let stream = streams.borrow_mut().remove(stream_id);
            if let Some(stream) = stream {
                let _guard = rt.enter();
                drop(stream);
            }
            Ok(Value::Null)
        }
        _ => Err(RpcError::method_not_found(method)),
    }
}
fn main() {
    let rt = Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("SMB runtime");
    let streams: Streams = Rc::new(RefCell::new(HashMap::new()));
    let next_stream = Cell::new(0u64);
    if let Err(e) = sdk::serve_with_manifest(
        "org.fiex.smb",
        include_str!("../manifest.json"), move |method, params| {
        dispatch(method, params, &rt, &streams, &next_stream)
    }) {
        eprintln!("SMB plugin IPC error: {e}");
        std::process::exit(1);
    }
}

async fn smb_client(conn: &StoredConnection) -> Result<(smb::Client, smb::UncPath), String> {
    let mut config = smb::ClientConfig::default();
    if let Some(port) = conn.port {
        config.connection.port = Some(port);
    }
    let client = smb::Client::new(config);
    let unc = smb::UncPath::from_str(&format!("\\\\{}\\{}", conn.host, conn.share))
        .map_err(|e| e.to_string())?;
    client
        .share_connect(&unc, &conn.username, conn.password.clone())
        .await
        .map_err(|e| format!("SMB 连接失败：{e}"))?;
    Ok((client, unc))
}

fn smb_path(conn: &StoredConnection, rel: &str) -> String {
    let joined = remote_join(&conn.base, rel).replace('/', "\\");
    joined.trim_matches('\\').to_string()
}

async fn smb_list(conn: &StoredConnection, rel: &str) -> Result<Vec<RemoteFile>, String> {
    let (client, unc) = smb_client(conn).await?;
    let path = smb_path(conn, rel);
    let target = if path.is_empty() {
        unc.clone()
    } else {
        unc.with_path(&path)
    };
    let dir = client
        .create_file(
            &target,
            &smb::FileCreateArgs::make_open_existing(
                smb::DirAccessMask::new()
                    .with_list_directory(true)
                    .with_synchronize(true)
                    .into(),
            ),
        )
        .await
        .map_err(|e| e.to_string())?
        .unwrap_dir();
    let dir = std::sync::Arc::new(dir);
    let stream =
        smb::Directory::query_with_options::<smb::FileFullDirectoryInformation>(&dir, "*", 65536)
            .await
            .map_err(|e| e.to_string())?;
    pin_mut!(stream);
    let mut out = Vec::new();
    while let Some(item) = stream.next().await {
        let item = item.map_err(|e| e.to_string())?;
        let name = item.file_name.to_string();
        if name == "." || name == ".." {
            continue;
        }
        out.push(RemoteFile {
            name,
            is_dir: item.file_attributes.directory(),
            size: item.end_of_file,
            modified: 0,
        });
    }
    let _ = dir.close().await;
    Ok(out)
}

/// Return at most max bytes. Preview content never escapes the plugin process
/// as a raw host filesystem path.
async fn smb_read_small(conn: &StoredConnection, rel: &str, max: usize) -> Result<Vec<u8>, String> {
    let (client, unc) = smb_client(conn).await?;
    let path = smb_path(conn, rel);
    let file = client
        .create_file(
            &unc.with_path(&path),
            &smb::FileCreateArgs::make_open_existing(
                smb::FileAccessMask::new().with_generic_read(true),
            ),
        )
        .await
        .map_err(|e| e.to_string())?
        .unwrap_file();
    let mut bytes = vec![0u8; max];
    let result = file.read_at(&mut bytes, 0).await.map_err(|e| e.to_string());
    let _ = file.close().await;
    bytes.truncate(result?);
    Ok(bytes)
}
