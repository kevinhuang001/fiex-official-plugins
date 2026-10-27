use super::{
    client::{self, Session},
    fs,
};
use fiex_plugin_sdk::{self as sdk, RpcError};
use serde_json::{json, Value};
fn fail(e: impl std::fmt::Display) -> RpcError {
    RpcError::internal(e.to_string())
}
fn stat(ctx: &sdk::RequestContext, session: &Session, node: &str) -> Result<Value, RpcError> {
    let cfg = client::config(ctx, ctx.params().get("connection"))?;
    let object = client::authorized(
        ctx,
        &cfg,
        session,
        "/api/fs/get",
        &json!({"path":format!("/{node}"),"password":""}),
    )?;
    let data = &object["data"];
    let revision = json!([
        data["modified"],
        data["size"],
        data["hashinfo"],
        data["hash_info"]
    ])
    .to_string();
    Ok(json!({"revision":revision,"size":data["size"]}))
}
pub(crate) fn dispatch(ctx: &sdk::RequestContext, session: &Session) -> Result<Value, RpcError> {
    let p = ctx.params();
    let node = p["nodeId"].as_str().ok_or_else(|| fail("missing nodeId"))?;
    fs::safe_relative(node)?;
    let before = stat(ctx, session, node)?;
    if ctx.method() == "fs.stat" {
        return Ok(before);
    }
    if before["revision"] != p["ifRevision"] {
        return Err(fail("同步冲突：远端已修改"));
    }
    let cfg = client::config(ctx, p.get("connection"))?;
    let token = client::token(ctx, &cfg, session)?;
    let reader = ctx
        .files()
        .open_read(&serde_json::from_value::<fiex_plugin_sdk::ResourceRef>(p["source"].clone()).map_err(|error| fail(error.to_string()))?)
        .map_err(fail)?;
    let response: Value = ctx
        .http()
        .put(&format!("{}/api/fs/put", cfg.url))
        .header("Authorization", token)
        .header(
            "File-Path",
            urlencoding::encode(&format!("/{node}")).into_owned(),
        )
        .header("As-Task", "false")
        .header("Content-Type", "application/octet-stream")
        .body_reader(reader)
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.json())
        .map_err(fail)?;
    if response["code"].as_u64() != Some(200) {
        return Err(fail(format!(
            "OpenList upload failed: {}",
            response["message"]
        )));
    }
    stat(ctx, session, node)
}
