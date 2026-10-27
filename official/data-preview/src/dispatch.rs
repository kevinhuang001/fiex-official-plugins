//! RPC dispatch over the open-file cache.
#[path = "../../../support/resources.rs"]
mod resources;
use crate::open::{open, page_of, Cache};
use fiex_plugin_sdk as sdk;
use fiex_plugin_sdk::RpcError;
use serde_json::Value;
use std::sync::Mutex;

fn info_rows(ctx: &sdk::RequestContext, target: &sdk::ResourceContext) -> Result<Value, RpcError> {
    let bytes = ctx
        .files()
        .read(&target.reference)
        .map_err(|error| RpcError::internal(format!("读取文件失败: {error}")))?;
    let (value, _) = open(resources::name(target)?, bytes).map_err(RpcError::internal)?;
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

pub(crate) fn dispatch(
    ctx: sdk::RequestContext,
    cache: &Mutex<Option<Cache>>,
) -> Result<Value, RpcError> {
    let method = ctx.method();
    let p = ctx.params();
    match method {
        "data.info" => {
            let target = resources::input(&ctx, "path")?;
            info_rows(&ctx, &target)
        }
        "data.open" => {
            let target = resources::input(&ctx, "path")?;
            let bytes = ctx
                .files()
                .read(&target.reference)
                .map_err(|error| RpcError::internal(format!("读取文件失败: {error}")))?;
            let (value, mut opened) =
                open(resources::name(&target)?, bytes).map_err(RpcError::internal)?;
            opened.path = serde_json::to_string(&target.reference).unwrap_or_default();
            *cache
                .lock()
                .map_err(|_| RpcError::internal("preview cache unavailable"))? = Some(opened);
            Ok(value)
        }
        "data.page" => {
            let target = resources::input(&ctx, "path")?;
            let dataset = p.get("dataset").and_then(Value::as_str).unwrap_or("data");
            let mut cached = cache
                .lock()
                .map_err(|_| RpcError::internal("preview cache unavailable"))?;
            if cached
                .as_ref()
                .map(|entry| {
                    entry.path != serde_json::to_string(&target.reference).unwrap_or_default()
                })
                .unwrap_or(true)
            {
                let bytes = ctx
                    .files()
                    .read(&target.reference)
                    .map_err(|error| RpcError::internal(format!("读取文件失败: {error}")))?;
                let (_, mut opened) =
                    open(resources::name(&target)?, bytes).map_err(RpcError::internal)?;
                opened.path = serde_json::to_string(&target.reference).unwrap_or_default();
                *cached = Some(opened);
            }
            let entry = cached.as_ref().unwrap();
            page_of(&entry.source, dataset, p).map_err(RpcError::internal)
        }
        _ => Err(RpcError::method_not_found(method)),
    }
}

#[path = "../../../support/provider_results.rs"]
mod provider_results;
pub(crate) struct Presentation;
impl sdk::InfoProvider for Presentation {
    fn resolve(&self, ctx: &sdk::RequestContext) -> Result<Vec<sdk::InfoSection>, RpcError> {
        let target = resources::input(ctx, "path")?;
        let rows = info_rows(ctx, &target)?;
        provider_results::section("data", "数据表", rows["rows"].clone())
    }
}
impl sdk::PreviewProvider for Presentation {
    fn open(&self, _ctx: &sdk::RequestContext) -> Result<sdk::PreviewView, RpcError> {
        Ok(sdk::PreviewView::html("ui/index.html"))
    }
}
