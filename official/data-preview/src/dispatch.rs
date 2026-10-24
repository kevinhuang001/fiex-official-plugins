//! RPC dispatch over the open-file cache.
use crate::open::{open, page_of, Cache};
use fiex_plugin_sdk as sdk;
use fiex_plugin_sdk::RpcError;
use serde_json::Value;
use std::sync::Mutex;

pub(crate) fn dispatch(
    ctx: sdk::RequestContext,
    cache: &Mutex<Option<Cache>>,
) -> Result<Value, RpcError> {
    let method = ctx.method();
    let p = ctx.params();
    match method {
        "data.info" => {
            let path = p
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| RpcError::invalid_params("missing path"))?;
            let bytes = ctx
                .files()
                .read(path)
                .map_err(|error| RpcError::internal(format!("读取文件失败: {error}")))?;
            let (value, _) = open(path, bytes).map_err(RpcError::internal)?;
            let datasets = value
                .get("datasets")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let page = value.get("page").cloned().unwrap_or(Value::Null);
            let columns = page
                .get("columns")
                .and_then(Value::as_array)
                .map(|items| items.len())
                .or_else(|| {
                    value
                        .pointer("/summary/columns")
                        .and_then(Value::as_u64)
                        .map(|count| count as usize)
                })
                .unwrap_or(0);
            let rows = page
                .get("total")
                .and_then(Value::as_u64)
                .or_else(|| value.pointer("/summary/rows").and_then(Value::as_u64))
                .unwrap_or(0);
            Ok(serde_json::json!({"rows": [
                {"label":"格式","value":value.get("format").and_then(Value::as_str).unwrap_or("").to_ascii_uppercase()},
                {"label":"数据集","value":datasets.len().to_string()},
                {"label":"首表列数","value":columns.to_string()},
                {"label":"首表行数","value":rows.to_string()},
            ]}))
        }
        "data.open" => {
            let path = p
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| RpcError::invalid_params("missing path"))?;
            let bytes = ctx
                .files()
                .read(path)
                .map_err(|error| RpcError::internal(format!("读取文件失败: {error}")))?;
            let (value, opened) = open(path, bytes).map_err(RpcError::internal)?;
            *cache
                .lock()
                .map_err(|_| RpcError::internal("preview cache unavailable"))? = Some(opened);
            Ok(value)
        }
        "data.page" => {
            let path = p
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| RpcError::invalid_params("missing path"))?;
            let dataset = p.get("dataset").and_then(Value::as_str).unwrap_or("data");
            let mut cached = cache
                .lock()
                .map_err(|_| RpcError::internal("preview cache unavailable"))?;
            if cached
                .as_ref()
                .map(|entry| entry.path != path)
                .unwrap_or(true)
            {
                let bytes = ctx
                    .files()
                    .read(path)
                    .map_err(|error| RpcError::internal(format!("读取文件失败: {error}")))?;
                let (_, opened) = open(path, bytes).map_err(RpcError::internal)?;
                *cached = Some(opened);
            }
            let entry = cached.as_ref().unwrap();
            page_of(&entry.source, dataset, p).map_err(RpcError::internal)
        }
        _ => Err(RpcError::method_not_found(method)),
    }
}
