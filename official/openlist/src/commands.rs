//! JSON-RPC method dispatch for the OpenList adapter.
use super::client::Session;
use super::fs::{self, Reads};
use fiex_plugin_sdk::{self as sdk, RpcError};
use serde_json::Value;

pub(crate) fn dispatch(
    method: &str,
    p: Value,
    session: &Session,
    reads: &Reads,
    host: &mut dyn sdk::HostServices,
) -> Result<Value, RpcError> {
    if method == "fs.closeRead" {
        fs::close_read(&p, reads);
        return Ok(Value::Null);
    }
    match method {
        "connection.validate" | "fs.list" => fs::list(method, &p, session, host),
        "fs.readSmall" | "fs.openRead" => fs::read(method, &p, session, reads, host),
        "fs.readStream" => fs::read_stream(&p, reads),
        _ => Err(RpcError::method_not_found(method)),
    }
}
