//! JSON-RPC method dispatch for the OpenList adapter.
use super::client::Session;
use super::fs::{self, Reads};
use fiex_plugin_sdk::{self as sdk, RpcError};
use serde_json::Value;

pub(crate) fn dispatch(
    ctx: sdk::RequestContext,
    session: &Session,
    reads: &Reads,
) -> Result<Value, RpcError> {
    let method = ctx.method();
    if method == "connection.open" {
        let mount = ctx.params()["mountId"]
            .as_str()
            .ok_or_else(|| RpcError::invalid_params("missing mountId"))?;
        return Ok(
            serde_json::json!({"kind":"vfs","mountId":mount,"nodeRef":{"mountId":mount,"nodeId":""}}),
        );
    }
    if matches!(method, "fs.stat" | "fs.writeFile") {
        return super::sync::dispatch(&ctx, session);
    }
    let p = ctx.params();
    if method == "fs.closeRead" {
        fs::close_read(p, reads);
        return Ok(Value::Null);
    }
    match method {
        "connection.validate" | "fs.list" => fs::list(method, p, session, &ctx),
        "fs.readSmall" | "fs.openRead" => fs::read(method, p, session, reads, &ctx),
        "fs.readStream" => fs::read_stream(p, reads, &ctx),
        _ => Err(RpcError::method_not_found(method)),
    }
}
