//! Read-only Word / Excel / PowerPoint preview.
//!
//! Parsing is delegated to `office_oxide`, which reads the OOXML package and
//! renders structured HTML: headings, paragraphs, lists, tables and sheet /
//! slide boundaries. The plugin only sanitizes and styles that HTML; it never
//! walks the XML by hand. Charts, SmartArt, animations, embedded objects and
//! legacy `.doc/.xls/.ppt` remain out of scope.
mod render;
#[path = "../../../support/resources.rs"]
mod resources;
mod sanitize;

use fiex_plugin_sdk::{self as sdk, RpcError};
use serde_json::Value;

fn info_rows(ctx: &sdk::RequestContext, target: &sdk::ResourceContext) -> Result<Value, RpcError> {
    let bytes = ctx
        .files()
        .read(&target.reference)
        .map_err(|error| RpcError::internal(format!("读取文件失败: {error}")))?;
    let extension = render::extension(resources::name(target)?);
    let value = render::render(bytes, &extension)?;
    let html = value["html"].as_str().unwrap_or("");
    let count = |needle: &str| html.matches(needle).count();
    Ok(serde_json::json!({"rows": [
        {"label":"格式","value":extension.to_ascii_uppercase()},
        {"label":"段落","value":count("<p").to_string()},
        {"label":"标题","value":count("<h1").to_string()},
        {"label":"表格","value":count("<table").to_string()},
        {"label":"图片","value":count("<img").to_string()},
        {"label":"列表项","value":count("<li").to_string()},
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
            let path = resources::name(&target)?;
            let bytes = ctx
                .files()
                .read(&target.reference)
                .map_err(|error| RpcError::internal(format!("读取文件失败: {error}")))?;
            render::render(bytes, &render::extension(path))
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
        provider_results::section("office", "文档", rows["rows"].clone())
    }
}
impl sdk::PreviewProvider for Presentation {
    fn open(&self, _ctx: &sdk::RequestContext) -> Result<sdk::PreviewView, RpcError> {
        Ok(sdk::PreviewView::html("ui/index.html"))
    }
}
