//! WebDAV filesystem handlers: directory listing, ranged reads and read
//! streams. The host owns caching and conflict resolution; this module only
//! translates Fiex RPC calls into WebDAV requests.
use super::client::{self, StoredConnection, DAV_BODY};
use fiex_plugin_sdk::{self as sdk, RpcError};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::Read,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

pub(crate) struct ReadStream {
    pub(crate) connection: StoredConnection,
    pub(crate) node_id: String,
}
pub(crate) type Streams = Arc<Mutex<HashMap<String, ReadStream>>>;

pub(crate) fn safe_relative(value: &str) -> Result<(), RpcError> {
    if value.starts_with('/')
        || value
            .split('/')
            .any(|p| p == "." || p == ".." || p.contains(['\\', '\0']))
    {
        return Err(RpcError::invalid_params("invalid node ID"));
    }
    Ok(())
}

pub(crate) fn list(
    host: &sdk::RequestContext,
    p: &Value,
    cfg: &StoredConnection,
    path: &str,
    method: &str,
) -> Result<Value, RpcError> {
    let http = host.http();
    let url = format!("{}/", client::webdav_url(cfg, path).trim_end_matches('/'));
    let xml = client::request(&http, cfg, "PROPFIND", &url)
        .header("Depth", "1")
        .header("Content-Type", "application/xml; charset=utf-8")
        .body(DAV_BODY)
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.text())
        .map_err(|e| RpcError::internal(e.to_string()))?;
    let mut nodes = client::parse_dav(&url, path, &xml).map_err(RpcError::internal)?;
    nodes.sort_by(|a, b| a.name.cmp(&b.name));
    if method == "connection.validate" {
        return Ok(json!({"valid":true}));
    }
    let start = p
        .get("cursor")
        .and_then(Value::as_str)
        .unwrap_or("0")
        .parse::<usize>()
        .map_err(|e| RpcError::invalid_params(e.to_string()))?;
    if start > nodes.len() {
        return Err(RpcError::invalid_params("cursor out of range"));
    }
    let limit = p
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(200)
        .clamp(1, 500) as usize;
    let end = start.saturating_add(limit).min(nodes.len());
    let entries = nodes[start..end]
        .iter()
        .map(|f| {
            json!({
                "nodeId": if path.is_empty() { f.path.clone() } else { format!("{}/{}", path.trim_matches('/'), f.path) },
                "name": f.name,
                "kind": if f.is_dir { "directory" } else { "file" },
                "size": f.size,
                "modified": f.modified,
                "revision": f.revision
            })
        })
        .collect::<Vec<_>>();
    Ok(
        json!({"entries":entries,"nextCursor":if end<nodes.len(){Some(end.to_string())}else{None},"snapshotToken":null,"identityStability":"path"}),
    )
}

pub(crate) fn open_read(
    streams: &Streams,
    next: &AtomicU64,
    cfg: StoredConnection,
    path: String,
) -> Result<Value, RpcError> {
    let mut streams = streams
        .lock()
        .map_err(|_| RpcError::internal("stream registry unavailable"))?;
    if streams.len() >= 32 {
        return Err(RpcError::internal("too many read streams"));
    }
    let id = format!("dav-read-{}", next.fetch_add(1, Ordering::Relaxed) + 1);
    streams.insert(
        id.clone(),
        ReadStream {
            connection: cfg,
            node_id: path,
        },
    );
    Ok(json!({"streamId":id}))
}

pub(crate) fn read(
    host: &sdk::RequestContext,
    p: &Value,
    cfg: &StoredConnection,
    path: &str,
) -> Result<Value, RpcError> {
    let http = host.http();
    let offset = p.get("offset").and_then(Value::as_u64).unwrap_or(0);
    let max = p
        .get("maxBytes")
        .and_then(Value::as_u64)
        .unwrap_or(262144)
        .clamp(1, 1048576);
    let end = offset
        .checked_add(max - 1)
        .ok_or_else(|| RpcError::invalid_params("range overflow"))?;
    let mut response = client::request(&http, cfg, "GET", &client::webdav_url(cfg, path))
        .header("Range", format!("bytes={offset}-{end}"))
        .send()
        .map_err(|e| RpcError::internal(e.to_string()))?;
    if response.status == 416 {
        return Ok(json!({"base64":"","bytesRead":0,"length":0,"eof":true}));
    }
    if response.status != 206 && !(offset == 0 && response.status == 200) {
        return Err(RpcError::internal("server did not respect byte range"));
    }
    let eof = if response.status == 206 {
        let header = response
            .headers
            .get("content-range")
            .ok_or_else(|| RpcError::internal("missing Content-Range"))?;
        let (bounds, total) = header
            .strip_prefix("bytes ")
            .and_then(|v| v.split_once('/'))
            .ok_or_else(|| RpcError::internal("invalid Content-Range"))?;
        let (start, last) = bounds
            .split_once('-')
            .ok_or_else(|| RpcError::internal("invalid Content-Range"))?;
        let start = start
            .parse::<u64>()
            .map_err(|e| RpcError::internal(e.to_string()))?;
        let last = last
            .parse::<u64>()
            .map_err(|e| RpcError::internal(e.to_string()))?;
        let total = total
            .parse::<u64>()
            .map_err(|e| RpcError::internal(e.to_string()))?;
        if start != offset || last < start || last - start >= max || last >= total {
            return Err(RpcError::internal("unexpected Content-Range"));
        }
        last + 1 == total
    } else {
        true
    };
    let mut bytes = Vec::new();
    (&mut response)
        .take(max + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| RpcError::internal(e.to_string()))?;
    if bytes.len() as u64 > max {
        return Err(RpcError::internal("response exceeds requested range"));
    }
    use base64::Engine;
    Ok(
        json!({"base64":base64::engine::general_purpose::STANDARD.encode(&bytes),"length":bytes.len(),"bytesRead":bytes.len(),"eof":eof}),
    )
}
