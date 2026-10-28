//! Read-only Word / Excel / PowerPoint preview.
//!
//! Parsing is delegated to `office_oxide`, which reads the OOXML package and
//! renders structured HTML: headings, paragraphs, lists, tables and sheet /
//! slide boundaries. The plugin only sanitizes and styles that HTML; it never
//! walks the XML by hand. Charts, SmartArt, animations, embedded objects and
//! legacy `.doc/.xls/.ppt` remain out of scope.
#[path = "../../../support/i18n.rs"]
mod i18n;
mod render;
#[path = "../../../support/resources.rs"]
mod resources;
mod sanitize;

use fiex_plugin_sdk::{self as sdk, RpcError};
use serde_json::Value;
use std::sync::{Mutex, OnceLock};

fn cache() -> &'static Mutex<Option<(String, Value)>> {
    static CACHE: OnceLock<Mutex<Option<(String, Value)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}
fn cached_render(
    ctx: &sdk::RequestContext,
    target: &sdk::ResourceContext,
) -> Result<Value, RpcError> {
    let pages = ctx
        .storage()
        .scoped("global")
        .get::<Value>("previewPages")?
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_f64().map(|number| number as u64))
        })
        .unwrap_or(6)
        .clamp(1, 30) as usize;
    let metadata = ctx
        .files()
        .metadata(&target.reference)
        .map_err(|error| RpcError::internal(error.to_string()))?;
    let key = serde_json::to_string(&(&target.reference, metadata.modified, metadata.size, pages))
        .map_err(|error| RpcError::internal(error.to_string()))?;
    if let Some(value) = cache().lock().ok().and_then(|entry| {
        entry
            .as_ref()
            .filter(|(old, _)| old == &key)
            .map(|(_, value)| value.clone())
    }) {
        return Ok(value);
    }
    let bytes = ctx
        .files()
        .read(&target.reference)
        .map_err(|error| RpcError::internal(error.to_string()))?;
    let value = render::render(bytes, &render::extension(resources::name(target)?), pages)?;
    *cache()
        .lock()
        .map_err(|_| RpcError::internal("preview cache unavailable"))? = Some((key, value.clone()));
    Ok(value)
}

fn info_rows(ctx: &sdk::RequestContext, target: &sdk::ResourceContext) -> Result<Value, RpcError> {
    let extension = render::extension(resources::name(target)?);
    let value = cached_render(ctx, target)?;
    let html = value["html"].as_str().unwrap_or("");
    let count = |needle: &str| html.matches(needle).count();
    Ok(serde_json::json!({"rows": [
        {"label":crate::i18n::t("格式"),"value":extension.to_ascii_uppercase()},
        {"label":crate::i18n::t("段落"),"value":count("<p").to_string()},
        {"label":crate::i18n::t("标题"),"value":count("<h1").to_string()},
        {"label":crate::i18n::t("表格"),"value":count("<table").to_string()},
        {"label":crate::i18n::t("图片"),"value":count("<img").to_string()},
        {"label":crate::i18n::t("列表项"),"value":count("<li").to_string()},
    ]}))
}

fn dispatch(ctx: sdk::RequestContext) -> Result<Value, RpcError> {
    let method = ctx.method();
    match method {
        "office.info" => {
            let target = resources::input(&ctx, "path")?;
            info_rows(&ctx, &target)
        }
        "office.render" => {
            let target = resources::input(&ctx, "path")?;
            cached_render(&ctx, &target)
        }
        _ => Err(RpcError::method_not_found(method)),
    }
}

fn main() {
    if let Err(error) = sdk::Providers::new()
        .info(Presentation)
        .preview(Presentation)
        .serve_with("org.fiex.office-preview", dispatch)
    {
        eprintln!("Office preview IPC failed: {error}");
        std::process::exit(1);
    }
}

#[path = "../../../support/provider_results.rs"]
mod provider_results;
pub(crate) struct Presentation;
impl sdk::InfoProvider for Presentation {
    fn resolve(&self, ctx: &sdk::RequestContext) -> Result<Vec<sdk::InfoSection>, RpcError> {
        let target = resources::input(ctx, "path")?;
        let rows = info_rows(ctx, &target)?;
        provider_results::section("office", crate::i18n::t("文档"), rows["rows"].clone())
    }
}
impl sdk::PreviewProvider for Presentation {
    fn open(&self, _ctx: &sdk::RequestContext) -> Result<sdk::PreviewView, RpcError> {
        Ok(sdk::PreviewView::html("ui/dist/index.html"))
    }
}
