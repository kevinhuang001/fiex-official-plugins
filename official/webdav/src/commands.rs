//! JSON-RPC method dispatch for the WebDAV adapter.
use super::client::StoredConnection;
use super::fs::{self, Streams};
use fiex_plugin_sdk::{self as sdk, RpcError};
use serde_json::Value;
use std::cell::Cell;

pub(crate) fn dispatch(
    method: &str,
    p: Value,
    streams: &Streams,
    next: &Cell<u64>,
    host: &mut dyn sdk::HostServices,
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
    let id = p.get("streamId").and_then(Value::as_str).unwrap_or("");
    if method == "fs.closeRead" {
        streams.borrow_mut().remove(id);
        return Ok(Value::Null);
    }
    let (cfg, path) = if method == "fs.readStream" {
        let streams = streams.borrow();
        let stream = streams
            .get(id)
            .ok_or_else(|| RpcError::invalid_params("unknown stream"))?;
        (stream.connection.clone(), stream.node_id.clone())
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
        fs::safe_relative(&path)?;
        (cfg, path)
    };
    match method {
        "connection.validate" | "fs.list" => fs::list(host, &p, &cfg, &path, method),
        "fs.openRead" => fs::open_read(streams, next, cfg, path),
        _ => fs::read(host, &p, &cfg, &path),
    }
}
