//! OpenList writes: the SDK feeds the body into a standard `Write`; `commit`
//! performs the single PUT (no `fs.readSmall`/`fs.writeFile` source grant).
use super::{
    client::{self, Session},
    fs,
};
use fiex_plugin_sdk::{
    filesystem::{FsRequest, FsWrite},
    http::StreamingBody,
    RpcError,
};
use serde_json::Value;
use std::{
    io,
    sync::{Arc, Mutex},
};

fn fail(e: impl std::fmt::Display) -> RpcError {
    RpcError::internal(e.to_string())
}

/// Bytes go straight from `vfs.writeStream` chunks into Core's HTTP worker.
struct BodyWriter {
    body: Arc<Mutex<Option<StreamingBody>>>,
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

/// The provider accepts both overwrite (`nodeId`) and creation
/// (`parentId` + `name`); Core sends the latter for "new empty file".
fn target_node(request: &FsRequest<'_>) -> Result<String, RpcError> {
    if let Some(node) = request.node_id() {
        let node = node.to_owned();
        fs::safe_relative(&node)?;
        return Ok(node);
    }
    let name = request
        .name()
        .ok_or_else(|| fail("missing nodeId or name"))?;
    new_node(request.parent_id_or_root(), name)
}

fn new_node(parent: &str, name: &str) -> Result<String, RpcError> {
    let parent = parent.trim_matches('/');
    let node = if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}/{name}")
    };
    fs::safe_relative(&node)?;
    Ok(node)
}

pub(crate) fn write(request: &FsRequest<'_>, session: &Session) -> Result<FsWrite, RpcError> {
    let host = request.ctx();
    let cfg = client::config(request.connection())?;
    let node = target_node(request)?;
    let token = client::token(host, &cfg, session)?;
    if request.node_id().is_some() {
        let before = fs::stat_http(&host.http(), &cfg, &token, &node)?;
        if request.if_revision() != before.revision.as_deref() {
            return Err(fail(crate::i18n::t("同步冲突：远端已修改")));
        }
    } else if request.if_revision().is_some() {
        return Err(fail("new files cannot carry a revision"));
    }
    let http = host.http();
    let size = request.size().unwrap_or(0);
    let body = http
        .put(&format!("{}/api/fs/put", cfg.url))
        .header("Authorization", token.clone())
        .header(
            "File-Path",
            urlencoding::encode(&format!("/{node}")).into_owned(),
        )
        .header("As-Task", "false")
        .header("Content-Type", "application/octet-stream")
        .body_stream(size)
        .map_err(fail)?;
    let body = Arc::new(Mutex::new(Some(body)));
    let writer = BodyWriter {
        body: Arc::clone(&body),
    };
    Ok(FsWrite::with_commit(writer, move || {
        let body = {
            let mut guard = body.lock().map_err(|_| fail("upload body unavailable"))?;
            guard
                .take()
                .ok_or_else(|| fail("upload already committed"))?
        };
        let response: Value = body
            .finish()
            .and_then(|r| r.error_for_status())
            .and_then(|r| r.json())
            .map_err(fail)?;
        if response["code"].as_u64() != Some(200) {
            return Err(fail(format!(
                "OpenList upload failed: {}",
                response["message"]
            )));
        }
        fs::stat_http(&http, &cfg, &token, &node)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_node_prefers_node_id_and_creates_from_parent() {
        // The request type is supplied by the SDK and cannot be built in a
        // unit test, so exercise the pure composition rule.
        assert_eq!(new_node("folder", "note.txt").unwrap(), "folder/note.txt");
        assert_eq!(new_node("", "note.txt").unwrap(), "note.txt");
        assert_eq!(new_node("/", "note.txt").unwrap(), "note.txt");
        assert!(new_node("folder", "../escape").is_err());
    }
}
