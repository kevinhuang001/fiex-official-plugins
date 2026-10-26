//! Host-streamed HTTP writes; no plugin-local staging files or whole-file buffers.
use super::{
    client::{self, StoredConnection},
    fs,
};
use fiex_plugin_sdk::{self as sdk, RpcError};
use serde_json::{json, Value};
fn fail(e: impl std::fmt::Display) -> RpcError {
    RpcError::internal(e.to_string())
}
fn stat(ctx: &sdk::RequestContext, cfg: &StoredConnection, node: &str) -> Result<Value, RpcError> {
    let response = client::request(&ctx.http(), cfg, "HEAD", &client::webdav_url(cfg, node))
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(fail)?;
    let revision = response
        .headers
        .get("etag")
        .cloned()
        .filter(|s| !s.starts_with("W/"))
        .ok_or_else(|| fail("WebDAV server must supply a strong ETag for safe synchronization"))?;
    Ok(
        json!({"revision":revision,"size":response.headers.get("content-length").and_then(|s|s.parse::<u64>().ok())}),
    )
}
pub(crate) fn dispatch(ctx: &sdk::RequestContext) -> Result<Value, RpcError> {
    let p = ctx.params();
    let cfg: StoredConnection = serde_json::from_value(p["connection"].clone()).map_err(fail)?;
    let node = p["nodeId"].as_str().ok_or_else(|| fail("missing nodeId"))?;
    fs::safe_relative(node)?;
    let before = stat(ctx, &cfg, node)?;
    if ctx.method() == "fs.stat" {
        return Ok(before);
    }
    let revision = p["ifRevision"]
        .as_str()
        .ok_or_else(|| fail("missing revision"))?;
    if before["revision"].as_str() != Some(revision) {
        return Err(fail("同步冲突：远端已修改"));
    }
    let reader = ctx
        .files()
        .open_read(p["source"].as_str().ok_or_else(|| fail("missing source"))?)
        .map_err(fail)?;
    client::request(&ctx.http(), &cfg, "PUT", &client::webdav_url(&cfg, node))
        .header("If-Match", revision)
        .header("Content-Type", "application/octet-stream")
        .body_reader(reader)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(fail)?;
    stat(ctx, &cfg, node)
}
