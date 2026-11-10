//! Word / Excel / PowerPoint preview and PDF export.
//!
//! Parsing is delegated to `office_oxide`, which reads the OOXML package and
//! renders structured HTML: headings, paragraphs, lists, tables and sheet /
//! slide boundaries. The plugin only sanitizes and styles that HTML; it never
//! walks the XML by hand. Charts, SmartArt, animations, embedded objects and
//! legacy `.doc/.xls/.ppt` remain out of scope.
use fiex_plugin_sdk::ProviderInvocationExt;
mod context_menu;
#[path = "../../../support/i18n.rs"]
mod i18n;
mod pdf;
#[path = "../../../support/provider_results.rs"]
mod provider_results;
mod render;
#[path = "../../../support/resources.rs"]
mod resources;
mod sanitize;

use fiex_plugin_sdk::{self as sdk, InfoFieldKey, RpcError};
use serde_json::Value;
use std::io::Read;
use std::sync::{Mutex, OnceLock};

fn cache() -> &'static Mutex<Option<(String, Value)>> {
    static CACHE: OnceLock<Mutex<Option<(String, Value)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}
fn page_limit(ctx: &dyn sdk::ProviderInvocation) -> Result<usize, RpcError> {
    Ok(ctx
        .storage()
        .scoped("global")
        .get::<Value>("previewPages")?
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_f64().map(|number| number as u64))
        })
        .unwrap_or(6)
        .clamp(1, 30) as usize)
}

fn cached_render(
    ctx: &dyn sdk::ProviderInvocation,
    target: &sdk::ResourceContext,
) -> Result<Value, RpcError> {
    let pages = page_limit(ctx)?;
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

fn pdf_pages(
    ctx: &dyn sdk::ProviderInvocation,
    target: &sdk::ResourceContext,
) -> Result<Value, RpcError> {
    let extension = render::extension(resources::name(target)?);
    if extension != "docx" && extension != "pptx" {
        return Err(RpcError::invalid_params(
            "PDF pages are only used for docx/pptx",
        ));
    }
    let mut file = ctx
        .files()
        .open_read(&target.reference)
        .map_err(|error| RpcError::internal(error.to_string()))?;
    let mut bytes = Vec::new();
    std::io::Read::take(&mut file, 256 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| RpcError::internal(error.to_string()))?;
    if bytes.len() > 256 * 1024 * 1024 {
        return Err(RpcError::invalid_params("Office file is too large"));
    }
    render::render_pdf_pages(bytes, &extension, page_limit(ctx)?)
}

fn info_fields(
    ctx: &dyn sdk::ProviderInvocation,
    target: &sdk::ResourceContext,
) -> Result<Vec<(String, String, String)>, RpcError> {
    let extension = render::extension(resources::name(target)?);
    let value = cached_render(ctx, target)?;
    let html = value["html"].as_str().unwrap_or("");
    let count = |needle: &str| html.matches(needle).count();
    Ok(vec![
        (
            "format".into(),
            crate::i18n::t("格式").into(),
            extension.to_ascii_uppercase(),
        ),
        (
            "paragraphs".into(),
            crate::i18n::t("段落").into(),
            count("<p").to_string(),
        ),
        (
            "headings".into(),
            crate::i18n::t("标题").into(),
            count("<h1").to_string(),
        ),
        (
            "tables".into(),
            crate::i18n::t("表格").into(),
            count("<table").to_string(),
        ),
        (
            "images".into(),
            crate::i18n::t("图片").into(),
            count("<img").to_string(),
        ),
        (
            "listItems".into(),
            crate::i18n::t("列表项").into(),
            count("<li").to_string(),
        ),
    ])
}

fn dispatch(ctx: sdk::RequestContext) -> Result<Value, RpcError> {
    let method = ctx.method();
    match method {
        "office.render" => {
            let target = resources::request_input(&ctx, "path")?;
            cached_render(&ctx, &target)
        }
        "office.pdfPages" => {
            let target = resources::request_input(&ctx, "path")?;
            pdf_pages(&ctx, &target)
        }
        _ => Err(RpcError::method_not_found(method)),
    }
}

fn main() {
    if let Err(error) = sdk::Providers::new()
        .diagnostics()
        .info(Presentation)
        .preview(Presentation)
        .context_menu(context_menu::OfficeMenu)
        .serve_with("org.fiex.office-preview", dispatch)
    {
        eprintln!("Office IPC failed: transport closed ({:?})", error.kind());
        std::process::exit(1);
    }
}

pub(crate) struct Presentation;
impl sdk::InfoProvider for Presentation {
    fn compute(
        &self,
        ctx: &dyn sdk::ProviderInvocation,
        fields: &[InfoFieldKey],
    ) -> Result<Vec<sdk::InfoFieldResult>, RpcError> {
        for field in fields {
            if field.section_id != "office"
                || !matches!(
                    field.field_id.as_str(),
                    "format" | "paragraphs" | "headings" | "tables" | "images" | "listItems"
                )
            {
                return Err(RpcError::invalid_params("unknown Office Info field"));
            }
        }
        if fields.is_empty() {
            return Ok(Vec::new());
        }
        let target = resources::target(ctx)?;
        let values = if fields.iter().any(|field| field.field_id != "format") {
            info_fields(ctx, &target)?
                .into_iter()
                .map(|(id, _, value)| (id, value))
                .collect::<std::collections::HashMap<_, _>>()
        } else {
            std::collections::HashMap::new()
        };
        let format = || -> Result<String, RpcError> {
            Ok(render::extension(resources::name(&target)?).to_ascii_uppercase())
        };
        fields
            .iter()
            .map(|field| {
                let value = if field.field_id == "format" {
                    Some(format()?)
                } else {
                    values.get(&field.field_id).cloned()
                };
                value
                    .map(|value| provider_results::result("office", &field.field_id, value))
                    .ok_or_else(|| RpcError::internal("Office Info result missing"))
            })
            .collect()
    }
}
impl sdk::PreviewProvider for Presentation {
    fn open(&self, ctx: &dyn sdk::ProviderInvocation) -> Result<sdk::PreviewView, RpcError> {
        let target = resources::target(ctx)?;
        let extension = render::extension(resources::name(&target)?);
        if extension == "docx" || extension == "pptx" {
            return Ok(sdk::PreviewView::html("ui/dist/pdf.html"));
        }
        Ok(sdk::PreviewView::html("ui/dist/index.html"))
    }
}
