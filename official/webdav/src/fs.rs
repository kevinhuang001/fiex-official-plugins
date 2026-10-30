//! WebDAV filesystem provider: PROPFIND listings/metadata and body streams.
//!
//! Core owns mount configuration, caching and conflict resolution; this
//! provider translates the unified `vfs.*` requests into WebDAV calls.
use super::client::{self, StoredConnection, DAV_BODY};
use fiex_plugin_sdk::{
    self as sdk,
    filesystem::{FsEntry, FsListing, FsRead, FsRequest, FsWrite},
    RpcError,
};
use serde_json::{json, Value};
use std::{
    io,
    sync::{Arc, Mutex},
};

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

pub(crate) fn connection(request: &FsRequest<'_>) -> Result<StoredConnection, RpcError> {
    serde_json::from_value(
        request
            .connection()
            .cloned()
            .ok_or_else(|| RpcError::invalid_params("missing connection"))?,
    )
    .map_err(|error| RpcError::invalid_params(error.to_string()))
}

fn propfind(
    http: &sdk::HttpClient,
    cfg: &StoredConnection,
    url: &str,
    relative: &str,
    depth: &str,
) -> Result<Vec<client::RemoteFile>, RpcError> {
    let xml = client::request(http, cfg, "PROPFIND", url)
        .header("Depth", depth)
        .header("Content-Type", "application/xml; charset=utf-8")
        .body(DAV_BODY)
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.text())
        .map_err(|e| RpcError::internal(e.to_string()))?;
    client::parse_dav(url, relative, &xml).map_err(RpcError::internal)
}

/// A strong ETag is the only safe revision for conditional writes.
fn strong_etag(value: &str) -> Option<String> {
    (!value.is_empty() && !value.starts_with("W/")).then(|| value.to_string())
}

fn entry(node: &client::RemoteFile, node_id: String, fallback_name: &str) -> FsEntry {
    let name = if node.name.is_empty() {
        fallback_name
    } else {
        &node.name
    };
    let mut entry = if node.is_dir {
        FsEntry::directory(node_id, name)
    } else {
        FsEntry::file(node_id, name)
    };
    entry.size = Some(if node.is_dir { 0 } else { node.size });
    entry.modified = Some(node.modified as i64);
    entry.revision = strong_etag(&node.revision);
    if !node.is_dir {
        entry.extension = name.rsplit_once('.').map(|(_, ext)| ext.to_string());
    }
    entry
}

fn display_name(path: &str) -> &str {
    if path.is_empty() {
        "root"
    } else {
        path.trim_matches('/').rsplit('/').next().unwrap_or(path)
    }
}

pub(crate) fn validate(host: &sdk::RequestContext) -> Result<Value, RpcError> {
    let cfg: StoredConnection = serde_json::from_value(
        host.params()
            .get("connection")
            .cloned()
            .ok_or_else(|| RpcError::invalid_params("missing connection"))?,
    )
    .map_err(|error| RpcError::invalid_params(error.to_string()))?;
    let url = format!("{}/", client::webdav_url(&cfg, "").trim_end_matches('/'));
    propfind(&host.http(), &cfg, &url, "", "1")?;
    Ok(json!({"valid":true}))
}

pub(crate) fn list(request: &FsRequest<'_>) -> Result<FsListing, RpcError> {
    let host = request.ctx();
    let cfg = connection(request)?;
    let path = request.node_id().unwrap_or("").to_owned();
    safe_relative(&path)?;
    let url = format!("{}/", client::webdav_url(&cfg, &path).trim_end_matches('/'));
    let mut nodes = propfind(&host.http(), &cfg, &url, &path, "1")?;
    // parse_dav keeps the requested collection itself; a listing only carries
    // its children.
    nodes.retain(|f| !f.path.is_empty() && f.path.trim_matches('/') != path.trim_matches('/'));
    nodes.sort_by(|a, b| a.name.cmp(&b.name));
    let start = request
        .params()
        .get("cursor")
        .and_then(Value::as_str)
        .unwrap_or("0")
        .parse::<usize>()
        .map_err(|e| RpcError::invalid_params(e.to_string()))?;
    if start > nodes.len() {
        return Err(RpcError::invalid_params("cursor out of range"));
    }
    let limit = request
        .params()
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(200)
        .clamp(1, 500) as usize;
    let end = start.saturating_add(limit).min(nodes.len());
    let entries = nodes[start..end]
        .iter()
        .map(|f| {
            let node_id = if path.is_empty() {
                f.path.clone()
            } else {
                format!("{}/{}", path.trim_matches('/'), f.path)
            };
            entry(f, node_id, display_name(&f.path))
        })
        .collect::<Vec<_>>();
    Ok(FsListing {
        entries,
        next_cursor: (end < nodes.len()).then(|| end.to_string()),
        warnings: Vec::new(),
        truncated: false,
    })
}

pub(crate) fn stat(request: &FsRequest<'_>) -> Result<FsEntry, RpcError> {
    let cfg = connection(request)?;
    let path = request.node_id().unwrap_or("").to_owned();
    safe_relative(&path)?;
    stat_http(&request.ctx().http(), &cfg, &path)
}

fn stat_http(
    http: &sdk::HttpClient,
    cfg: &StoredConnection,
    path: &str,
) -> Result<FsEntry, RpcError> {
    let url = client::webdav_url(cfg, path);
    let nodes = propfind(http, cfg, &url, path, "0")?;
    let node = nodes
        .into_iter()
        .next()
        .ok_or_else(|| RpcError::internal("remote node not found"))?;
    Ok(entry(&node, path.to_string(), display_name(path)))
}

fn valid_name(name: &str) -> Result<(), RpcError> {
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\', ':', '\0']) {
        return Err(RpcError::invalid_params("invalid name"));
    }
    Ok(())
}

fn join_path(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{}/{}", parent.trim_matches('/'), name)
    }
}

fn split_path(node: &str) -> (&str, &str) {
    node.rsplit_once('/').unwrap_or(("", node))
}

pub(crate) fn mkdir(request: &FsRequest<'_>) -> Result<FsEntry, RpcError> {
    let cfg = connection(request)?;
    let parent = request.parent_id().unwrap_or("");
    safe_relative(parent)?;
    let name = request
        .name()
        .ok_or_else(|| RpcError::invalid_params("missing name"))?;
    valid_name(name)?;
    let path = join_path(parent, name);
    safe_relative(&path)?;
    let http = request.ctx().http();
    client::request(&http, &cfg, "MKCOL", &client::webdav_url(&cfg, &path))
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| RpcError::internal(e.to_string()))?;
    stat_http(&http, &cfg, &path)
}

pub(crate) fn rename(request: &FsRequest<'_>) -> Result<FsEntry, RpcError> {
    let cfg = connection(request)?;
    let node = request
        .node_id()
        .ok_or_else(|| RpcError::invalid_params("missing nodeId"))?;
    if node.is_empty() {
        return Err(RpcError::invalid_params("cannot rename the mount root"));
    }
    safe_relative(node)?;
    let (old_parent, old_name) = split_path(node);
    let new_name = request.name().unwrap_or(old_name);
    valid_name(new_name)?;
    let new_parent = request.parent_id().unwrap_or(old_parent);
    safe_relative(new_parent)?;
    let new_path = join_path(new_parent, new_name);
    safe_relative(&new_path)?;
    let http = request.ctx().http();
    if new_path == node {
        return stat_http(&http, &cfg, node);
    }
    let mut update = client::request(&http, &cfg, "MOVE", &client::webdav_url(&cfg, node))
        .header("Destination", client::webdav_url(&cfg, &new_path))
        .header("Overwrite", "F")
        .header("Depth", "infinity");
    if let Some(revision) = request.if_revision() {
        update = update.header("If-Match", revision);
    }
    update
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| RpcError::internal(e.to_string()))?;
    stat_http(&http, &cfg, &new_path)
}

pub(crate) fn remove(request: &FsRequest<'_>) -> Result<(), RpcError> {
    let cfg = connection(request)?;
    let node = request
        .node_id()
        .ok_or_else(|| RpcError::invalid_params("missing nodeId"))?;
    if node.is_empty() {
        return Err(RpcError::invalid_params("cannot remove the mount root"));
    }
    safe_relative(node)?;
    let http = request.ctx().http();
    let entry = stat_http(&http, &cfg, node)?;
    if entry.is_dir() {
        let url = format!("{}/", client::webdav_url(&cfg, node).trim_end_matches('/'));
        let nodes = propfind(&http, &cfg, &url, node, "1")?;
        let non_empty = nodes.iter().any(|child| {
            !child.path.is_empty() && child.path.trim_matches('/') != node.trim_matches('/')
        });
        if non_empty {
            return Err(RpcError::internal(crate::i18n::t("目录不为空，无法删除")));
        }
    }
    let mut delete = client::request(&http, &cfg, "DELETE", &client::webdav_url(&cfg, node));
    if let Some(revision) = request.if_revision() {
        delete = delete.header("If-Match", revision);
    }
    delete
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| RpcError::internal(e.to_string()))?;
    Ok(())
}

// WebDAV does not expose a reliable length in openRead. Launching a
// multi-connection staging engine for every document slows previews and small
// reads. Use the host HTTP response stream directly until Core can supply a
// trusted length for adaptive segmented downloads.
pub(crate) fn read(request: &FsRequest<'_>) -> Result<FsRead, RpcError> {
    let host = request.ctx();
    let cfg = connection(request)?;
    let path = request
        .node_id()
        .ok_or_else(|| RpcError::invalid_params("missing node ID"))?;
    safe_relative(path)?;
    let response = client::request(&host.http(), &cfg, "GET", &client::webdav_url(&cfg, path))
        .timeout(std::time::Duration::from_secs(3600))
        .send()
        .map_err(|e| RpcError::internal(e.to_string()))?;
    if response.status != 200 && response.status != 206 {
        return Err(RpcError::internal(format!(
            "WebDAV read HTTP {}",
            response.status
        )));
    }
    let size = response
        .headers
        .get("content-length")
        .and_then(|value| value.parse::<u64>().ok());
    Ok(FsRead::new(response, size))
}

/// Strong-ETag HEAD used by conditional writes and the commit result.
fn head(
    http: &sdk::HttpClient,
    cfg: &StoredConnection,
    node: &str,
) -> Result<(String, Option<u64>), RpcError> {
    let response = client::request(http, cfg, "HEAD", &client::webdav_url(cfg, node))
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| RpcError::internal(e.to_string()))?;
    let revision = response
        .headers
        .get("etag")
        .cloned()
        .filter(|s| !s.starts_with("W/"))
        .ok_or_else(|| {
            RpcError::internal("WebDAV server must supply a strong ETag for safe synchronization")
        })?;
    let size = response
        .headers
        .get("content-length")
        .and_then(|s| s.parse::<u64>().ok());
    Ok((revision, size))
}

/// The body streams straight to Core's HTTP worker; the commit callback only
/// finishes the request and re-reads the new revision.
struct BodyWriter {
    body: Arc<Mutex<Option<sdk::http::StreamingBody>>>,
}

impl io::Write for BodyWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut guard = self
            .body
            .lock()
            .map_err(|_| io::Error::other("upload body unavailable"))?;
        guard
            .as_mut()
            .ok_or_else(|| io::Error::other("upload already committed"))?
            .write_chunk(bytes)
            .map(|()| bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(crate) fn write(request: &FsRequest<'_>) -> Result<FsWrite, RpcError> {
    let host = request.ctx();
    let cfg = connection(request)?;
    let node = request
        .node_id()
        .ok_or_else(|| RpcError::invalid_params("missing nodeId"))?
        .to_owned();
    safe_relative(&node)?;
    let http = host.http();
    let (before, _) = head(&http, &cfg, &node)?;
    let revision = request
        .if_revision()
        .ok_or_else(|| RpcError::internal("missing revision"))?
        .to_string();
    if before != revision {
        return Err(RpcError::internal(crate::i18n::t("同步冲突：远端已修改")));
    }
    let url = client::webdav_url(&cfg, &node);
    let size = request.size().unwrap_or(0);
    let body = http
        .put(&url)
        .header("If-Match", revision.clone())
        .header("Content-Type", "application/octet-stream")
        .body_stream(size)
        .map_err(|error| RpcError::internal(error.to_string()))?;
    let body = Arc::new(Mutex::new(Some(body)));
    let writer = BodyWriter {
        body: Arc::clone(&body),
    };
    Ok(FsWrite::with_commit(writer, move || {
        let body = {
            let mut guard = body
                .lock()
                .map_err(|_| RpcError::internal("upload body unavailable"))?;
            guard
                .take()
                .ok_or_else(|| RpcError::internal("upload already committed"))?
        };
        body.finish()
            .and_then(|r| r.error_for_status())
            .map_err(|e| RpcError::internal(e.to_string()))?;
        let (revision, size) = head(&http, &cfg, &node)?;
        let name = display_name(&node).to_string();
        let mut entry = FsEntry::file(node.clone(), name);
        entry.size = size;
        entry.revision = Some(revision);
        Ok(entry)
    }))
}
