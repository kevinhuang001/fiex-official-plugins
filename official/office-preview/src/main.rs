//! Read-only Word / Excel / PowerPoint preview.
//!
//! Parsing is delegated to `office_oxide`, which reads the OOXML package and
//! renders structured HTML: headings, paragraphs, lists, tables and sheet /
//! slide boundaries. The plugin only sanitizes and styles that HTML; it never
//! walks the XML by hand. Charts, SmartArt, animations, embedded objects and
//! legacy `.doc/.xls/.ppt` remain out of scope.
use fiex_plugin_sdk::{self as sdk, RpcError};
use office_oxide::{Document, DocumentFormat};
use serde_json::{json, Value};
use std::io::Cursor;

/// Defense in depth: the renderer escapes text, but an iframe DOM is built from
/// this string, so drop anything executable before it reaches the UI.
fn sanitize(html: &str) -> String {
    let mut output = String::with_capacity(html.len());
    let lower = html.to_ascii_lowercase();
    let mut index = 0;
    while index < html.len() {
        if lower[index..].starts_with("<script")
            || lower[index..].starts_with("<iframe")
            || lower[index..].starts_with("<object")
            || lower[index..].starts_with("<embed")
        {
            let closing = if lower[index..].starts_with("<script") {
                "</script>"
            } else if lower[index..].starts_with("<iframe") {
                "</iframe>"
            } else if lower[index..].starts_with("<object") {
                "</object>"
            } else {
                "</embed>"
            };
            index = lower[index..]
                .find(closing)
                .map(|offset| index + offset + closing.len())
                .unwrap_or(html.len());
            continue;
        }
        let rest = &html[index..];
        let consumed = rest.chars().next().map(char::len_utf8).unwrap_or(1);
        if rest.starts_with("on")
            && rest
                .chars()
                .nth(2)
                .is_some_and(|next| next.is_ascii_alphabetic())
        {
            // Attribute handlers only appear inside tags; dropping the name is
            // enough because the tag itself is kept.
            if let Some(end) = rest.find(|c: char| c.is_whitespace() || c == '>' || c == '=') {
                index += end;
                continue;
            }
        }
        if rest.to_ascii_lowercase().starts_with("javascript:") {
            index += "javascript:".len();
            continue;
        }
        output.push_str(&html[index..index + consumed]);
        index += consumed;
    }
    output
}

fn format(extension: &str) -> Result<DocumentFormat, RpcError> {
    match extension {
        "docx" => Ok(DocumentFormat::Docx),
        "xlsx" => Ok(DocumentFormat::Xlsx),
        "pptx" => Ok(DocumentFormat::Pptx),
        _ => Err(RpcError::invalid_params("仅支持 docx、xlsx、pptx")),
    }
}

fn extension(path: &str) -> String {
    path.rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default()
}

fn render(bytes: Vec<u8>, extension: &str) -> Result<Value, RpcError> {
    let format = format(extension)?;
    let document = Document::from_reader(Cursor::new(bytes), format)
        .map_err(|error| RpcError::internal(format!("无法解析 Office 文件: {error}")))?;
    let html = sanitize(&document.to_html());
    if html.trim().is_empty() {
        return Ok(json!({ "kind": extension, "html": "", "empty": true }));
    }
    Ok(json!({ "kind": extension, "html": html }))
}

fn dispatch(method: &str, p: Value, host: &mut dyn sdk::HostServices) -> Result<Value, RpcError> {
    match method {
        "office.render" => {
            let path = p
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| RpcError::invalid_params("missing path"))?;
            let bytes = host
                .files()
                .read(path)
                .map_err(|error| RpcError::internal(format!("读取文件失败: {error}")))?;
            render(bytes, &extension(path))
        }
        _ => Err(RpcError::method_not_found(method)),
    }
}

fn main() {
    if let Err(error) = sdk::serve_with_host("org.fiex.office-preview", dispatch) {
        eprintln!("Office preview IPC failed: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(format!("tests/fixtures/{name}")).unwrap()
    }

    #[test]
    fn renders_docx_structure() {
        let value = render(fixture("fixture.docx"), "docx").unwrap();
        let html = value["html"].as_str().unwrap();
        assert!(html.contains("<h1>"), "{html}");
        assert!(html.contains("2026 年第三季度产品报告"));
        assert!(html.contains("<table>"));
        assert!(html.contains("1,200,000"));
        assert!(html.contains("<ul>"));
    }

    #[test]
    fn renders_xlsx_sheets() {
        let value = render(fixture("fixture.xlsx"), "xlsx").unwrap();
        let html = value["html"].as_str().unwrap();
        assert!(html.contains("预算"));
        assert!(html.contains("收入"));
        assert!(html.contains("<table>"));
        assert!(html.contains("1120000"));
    }

    #[test]
    fn renders_pptx_slides() {
        let value = render(fixture("fixture.pptx"), "pptx").unwrap();
        let html = value["html"].as_str().unwrap();
        assert!(html.contains("Fiex 产品介绍"));
        assert!(html.contains("插件化"));
        assert!(html.contains("下一步"));
    }

    #[test]
    fn rejects_unknown_and_corrupt_files() {
        assert!(render(Vec::new(), "docx").is_err());
        assert!(render(Vec::new(), "doc").is_err());
    }

    #[test]
    fn strips_executable_markup() {
        let html = sanitize(
            "<p onclick=\"x()\">a<script>alert(1)</script><img src=x onerror=alert(2)></p>",
        );
        assert!(!html.contains("<script"));
        assert!(!html.contains("onclick"));
        assert!(!html.contains("onerror"));
        assert!(html.contains("<img src=x"));
    }
}
