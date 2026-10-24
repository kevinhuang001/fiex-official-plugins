//! RPC dispatch over the open-file cache.
use crate::open::{open, page_of, Cache};
use fiex_plugin_sdk as sdk;
use fiex_plugin_sdk::RpcError;
use serde_json::Value;
use std::cell::RefCell;

pub(crate) fn dispatch(
    method: &str,
    p: Value,
    cache: &RefCell<Option<Cache>>,
    host: &mut dyn sdk::HostServices,
) -> Result<Value, RpcError> {
    match method {
        "data.open" => {
            let path = p
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| RpcError::invalid_params("missing path"))?;
            let bytes = host
                .files()
                .read(path)
                .map_err(|error| RpcError::internal(format!("读取文件失败: {error}")))?;
            let (value, opened) = open(path, bytes).map_err(RpcError::internal)?;
            *cache.borrow_mut() = Some(opened);
            Ok(value)
        }
        "data.page" => {
            let path = p
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| RpcError::invalid_params("missing path"))?;
            let dataset = p.get("dataset").and_then(Value::as_str).unwrap_or("data");
            let mut cached = cache.borrow_mut();
            if cached
                .as_ref()
                .map(|entry| entry.path != path)
                .unwrap_or(true)
            {
                let bytes = host
                    .files()
                    .read(path)
                    .map_err(|error| RpcError::internal(format!("读取文件失败: {error}")))?;
                let (_, opened) = open(path, bytes).map_err(RpcError::internal)?;
                *cached = Some(opened);
            }
            let entry = cached.as_ref().unwrap();
            page_of(&entry.source, dataset, &p).map_err(RpcError::internal)
        }
        _ => Err(RpcError::method_not_found(method)),
    }
}
