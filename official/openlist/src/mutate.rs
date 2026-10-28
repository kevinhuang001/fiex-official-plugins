//! OpenList directory mutations. All calls use the same authenticated session
//! as reads, and every operation re-checks the node state it mutates.
use super::{
    client::{self, Session},
    fs,
};
use fiex_plugin_sdk::{
    filesystem::{FsEntry, FsRequest},
    RpcError,
};
use serde_json::{json, Value};

fn fail(e: impl std::fmt::Display) -> RpcError {
    RpcError::internal(e.to_string())
}

pub(crate) fn mkdir(request: &FsRequest<'_>, session: &Session) -> Result<FsEntry, RpcError> {
    let host = request.ctx();
    let cfg = client::config(request.connection())?;
    let parent = request.parent_id().unwrap_or("");
    fs::safe_relative(parent)?;
    let name = request.name().ok_or_else(|| fail("missing name"))?;
    fs::valid_name(name)?;
    let path = fs::join_path(parent, name);
    fs::safe_relative(&path)?;
    client::authorized(
        host,
        &cfg,
        session,
        "/api/fs/mkdir",
        &json!({ "path": fs::api_path(&path), "password": "" }),
    )?;
    fs::stat_node(host, &cfg, session, &path)
}

pub(crate) fn rename(request: &FsRequest<'_>, session: &Session) -> Result<FsEntry, RpcError> {
    let host = request.ctx();
    let cfg = client::config(request.connection())?;
    let node = request.node_id().ok_or_else(|| fail("missing nodeId"))?;
    if node.is_empty() {
        return Err(fail("cannot rename the mount root"));
    }
    fs::safe_relative(node)?;
    let before = fs::stat_node(host, &cfg, session, node)?;
    if let Some(revision) = request.if_revision() {
        if before.revision.as_deref() != Some(revision) {
            return Err(fail(crate::i18n::t("同步冲突：远端已修改")));
        }
    }
    let (old_parent, old_name) = fs::split_path(node);
    let new_name = request.name().unwrap_or(old_name);
    fs::valid_name(new_name)?;
    let new_parent = request.parent_id().unwrap_or(old_parent);
    fs::safe_relative(new_parent)?;
    let new_path = fs::join_path(new_parent, new_name);
    fs::safe_relative(&new_path)?;
    if new_path == node {
        return Ok(before);
    }
    if new_parent == old_parent {
        client::authorized(
            host,
            &cfg,
            session,
            "/api/fs/rename",
            &json!({ "path": fs::api_path(node), "name": new_name, "overwrite": false }),
        )?;
    } else {
        client::authorized(
            host,
            &cfg,
            session,
            "/api/fs/move",
            &json!({
                "src_dir": fs::api_path(old_parent),
                "dst_dir": fs::api_path(new_parent),
                "names": [old_name],
                "overwrite": false
            }),
        )?;
    }
    fs::stat_node(host, &cfg, session, &new_path)
}

pub(crate) fn remove(request: &FsRequest<'_>, session: &Session) -> Result<(), RpcError> {
    let host = request.ctx();
    let cfg = client::config(request.connection())?;
    let node = request.node_id().ok_or_else(|| fail("missing nodeId"))?;
    if node.is_empty() {
        return Err(fail("cannot remove the mount root"));
    }
    fs::safe_relative(node)?;
    let entry = fs::stat_node(host, &cfg, session, node)?;
    if let Some(revision) = request.if_revision() {
        if entry.revision.as_deref() != Some(revision) {
            return Err(fail(crate::i18n::t("同步冲突：远端已修改")));
        }
    }
    // AList removes a directory recursively; refuse a non-empty tree instead
    // of silently deleting children.
    if entry.is_dir() {
        let parsed = client::authorized(
            host,
            &cfg,
            session,
            "/api/fs/list",
            &json!({
                "path": fs::api_path(node),
                "password": "",
                "page": 1,
                "per_page": 0,
                "refresh": false
            }),
        )?;
        let non_empty = parsed
            .pointer("/data/content")
            .and_then(Value::as_array)
            .is_some_and(|items| !items.is_empty());
        if non_empty {
            return Err(fail(crate::i18n::t("目录不为空，无法删除")));
        }
    }
    let (parent, name) = fs::split_path(node);
    client::authorized(
        host,
        &cfg,
        session,
        "/api/fs/remove",
        &json!({ "dir": fs::api_path(parent), "names": [name] }),
    )?;
    Ok(())
}
