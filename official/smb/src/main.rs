//! SMB protocol implementation. All socket operations are brokered by Core.
mod transport;
use fiex_plugin_sdk::{self as sdk, RpcError};
use serde::Deserialize;
use serde_json::{json, Value};
use smb::{GetLen, ReadAt};
use std::{collections::HashMap, str::FromStr};

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
#[derive(Clone)]
struct ReadStream {
    connection: StoredConnection,
    path: String,
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
    host: sdk::worker::HostProxy,
    cfg: StoredConnection,
    path: String,
    method: String,
    params: Value,
) -> Result<Value, RpcError> {
    let run = || -> Result<Value, String> {
        let port = cfg.port.unwrap_or(445);
        let transport =
            transport::HostTransport::open(host, &cfg.host, port).map_err(|e| e.to_string())?;
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
                let node = if path.is_empty() {
                    name.clone()
                } else {
                    format!("{}/{}", path.replace('\\', "/"), name)
                };
                entries.push(json!({"nodeId":node,"name":name,"kind":if item.file_attributes.directory(){"directory"}else{"file"},"size":item.end_of_file,"modified":0}));
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
            let result = if method == "fs.openRead" {
                json!({"size":total})
            } else {
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
                json!({"base64":base64::engine::general_purpose::STANDARD.encode(bytes),"length":read,"bytesRead":read,"eof":offset.saturating_add(read as u64)>=total})
            };
            let _ = file.close();
            result
        };
        let _ = tree.disconnect();
        let _ = connection.close();
        Ok(result)
    };
    run().map_err(RpcError::internal)
}
fn main() {
    let mut streams: HashMap<String, ReadStream> = HashMap::new();
    let mut sequence = 0u64;
    let result = sdk::serve_with_host("org.fiex.smb", move |method, p, host| {
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
            streams.remove(stream_id);
            return Ok(Value::Null);
        }
        let (cfg, path) = if method == "fs.readStream" {
            let stream = streams
                .get(stream_id)
                .ok_or_else(|| RpcError::invalid_params("unknown read stream"))?;
            (stream.connection.clone(), stream.path.clone())
        } else {
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
            (cfg, path)
        };
        let stored = ReadStream {
            connection: cfg.clone(),
            path: path.clone(),
        };
        let method_owned = method.to_owned();
        let mut value = sdk::worker::with_host_worker(host, move |proxy| {
            remote(proxy, cfg, path, method_owned, p)
        })?;
        if method == "fs.openRead" {
            if streams.len() >= 32 {
                return Err(RpcError::internal("too many read streams"));
            }
            sequence += 1;
            let id = format!("smb-read-{sequence}");
            streams.insert(id.clone(), stored);
            value["streamId"] = json!(id);
        }
        Ok(value)
    });
    if let Err(error) = result {
        eprintln!("SMB plugin IPC failed: {error}");
        std::process::exit(1);
    }
}
