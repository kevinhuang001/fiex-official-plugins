//! Read-only Word / Excel / PowerPoint preview.
//!
//! Parsing is delegated to `office_oxide`, which reads the OOXML package and
//! renders structured HTML: headings, paragraphs, lists, tables and sheet /
//! slide boundaries. The plugin only sanitizes and styles that HTML; it never
//! walks the XML by hand. Charts, SmartArt, animations, embedded objects and
//! legacy `.doc/.xls/.ppt` remain out of scope.
mod render;
mod sanitize;

use fiex_plugin_sdk::{self as sdk, RpcError};
use serde_json::Value;
use std::sync::Mutex;

const DEFAULT_PREVIEW_PAGES: usize = 6;
const MAX_PREVIEW_PAGES: usize = 30;

struct CacheEntry {
    path: String,
    modified: u64,
    size: u64,
    pages: usize,
    value: Value,
}

fn preview_pages(ctx: &sdk::RequestContext) -> usize {
    ctx.storage_get("global", "previewPages")
        .ok()
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_f64().map(|number| number as u64))
        })
        .map(|value| value.clamp(1, MAX_PREVIEW_PAGES as u64) as usize)
        .unwrap_or(DEFAULT_PREVIEW_PAGES)
}

fn cached_render(
    ctx: &sdk::RequestContext,
    path: &str,
    cache: &Mutex<Option<CacheEntry>>,
) -> Result<Value, RpcError> {
    let pages = preview_pages(ctx);
    let metadata = ctx.files().metadata(path).ok();
    let modified = metadata.as_ref().map(|entry| entry.modified).unwrap_or(0);
    let size = metadata.as_ref().map(|entry| entry.size).unwrap_or(0);
    if let Some(value) = cache.lock().ok().and_then(|entry| {
        entry
            .as_ref()
            .filter(|entry| {
                entry.path == path
                    && entry.modified == modified
                    && entry.size == size
                    && entry.pages == pages
            })
            .map(|entry| entry.value.clone())
    }) {
        return Ok(value);
    }
    let bytes = ctx
        .files()
        .read(path)
        .map_err(|error| RpcError::internal(format!("读取文件失败: {error}")))?;
    let value = render::render(bytes, &render::extension(path), pages)?;
    if let Ok(mut entry) = cache.lock() {
        *entry = Some(CacheEntry {
            path: path.to_string(),
            modified,
            size,
            pages,
            value: value.clone(),
        });
    }
    Ok(value)
}

fn info_rows(
    ctx: &sdk::RequestContext,
    path: &str,
    cache: &Mutex<Option<CacheEntry>>,
) -> Result<Value, RpcError> {
    let value = cached_render(ctx, path, cache)?;
    let html = value["html"].as_str().unwrap_or("");
    let count = |needle: &str| html.matches(needle).count();
    Ok(serde_json::json!({"rows": [
        {"label":"格式","value":render::extension(path).to_ascii_uppercase()},
        {"label":"段落","value":count("<p").to_string()},
        {"label":"标题","value":count("<h1").to_string()},
        {"label":"表格","value":count("<table").to_string()},
        {"label":"图片","value":count("<img").to_string()},
        {"label":"列表项","value":count("<li").to_string()},
    ]}))
}

fn dispatch(
    ctx: sdk::RequestContext,
    cache: &Mutex<Option<CacheEntry>>,
) -> Result<Value, RpcError> {
    let method = ctx.method();
    let p = ctx.params();
    if method == "info.resolve" {
        let name = ctx
            .targets()
            .first()
            .and_then(sdk::ResourceContext::name)
            .ok_or_else(|| RpcError::invalid_params("missing target metadata name"))?;
        let rows = info_rows(&ctx, name, cache)?;
        return Ok(serde_json::json!({
            "sections": [{"id":"office","title":"文档","rows": rows["rows"].clone()}]
        }));
    }
    if method == "preview.open" {
        return Ok(serde_json::json!({
            "kind": "html",
            "view": {"source":"package","entry":"ui/index.html"}
        }));
    }
    match method {
        "office.info" => {
            let path = p
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| RpcError::invalid_params("missing path"))?;
            info_rows(&ctx, path, cache)
        }
        "office.render" => {
            let path = p
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| RpcError::invalid_params("missing path"))?;
            cached_render(&ctx, path, cache)
        }
        _ => Err(RpcError::method_not_found(method)),
    }
}

fn main() {
    let cache = Mutex::new(None);
    if let Err(error) = sdk::serve("org.fiex.office-preview", move |ctx| dispatch(ctx, &cache)) {
        eprintln!("Office preview IPC failed: {error}");
        std::process::exit(1);
    }
}
