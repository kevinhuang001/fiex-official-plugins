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

fn dispatch(ctx: sdk::RequestContext) -> Result<Value, RpcError> {
    let method = ctx.method();
    match method {
        "office.info" => {
            let path = ctx
                .params()
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| RpcError::invalid_params("missing path"))?;
            let bytes = ctx
                .files()
                .read(path)
                .map_err(|error| RpcError::internal(format!("读取文件失败: {error}")))?;
            let extension = render::extension(path);
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
        "office.render" => {
            let path = ctx
                .params()
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| RpcError::invalid_params("missing path"))?;
            let bytes = ctx
                .files()
                .read(path)
                .map_err(|error| RpcError::internal(format!("读取文件失败: {error}")))?;
            render::render(bytes, &render::extension(path))
        }
        _ => Err(RpcError::method_not_found(method)),
    }
}

fn main() {
    if let Err(error) = sdk::serve("org.fiex.office-preview", dispatch) {
        eprintln!("Office preview IPC failed: {error}");
        std::process::exit(1);
    }
}
