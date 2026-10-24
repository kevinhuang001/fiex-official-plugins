//! Format detection and rendering: parse the OOXML package with `office_oxide`
//! and sanitize the HTML it produces.
use crate::sanitize::sanitize;
use fiex_plugin_sdk::RpcError;
use office_oxide::{Document, DocumentFormat};
use serde_json::{json, Value};
use std::io::Cursor;

fn format(extension: &str) -> Result<DocumentFormat, RpcError> {
    match extension {
        "docx" => Ok(DocumentFormat::Docx),
        "xlsx" => Ok(DocumentFormat::Xlsx),
        "pptx" => Ok(DocumentFormat::Pptx),
        _ => Err(RpcError::invalid_params("仅支持 docx、xlsx、pptx")),
    }
}

pub(crate) fn extension(path: &str) -> String {
    path.rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default()
}

pub(crate) fn render(bytes: Vec<u8>, extension: &str) -> Result<Value, RpcError> {
    let format = format(extension)?;
    let document = Document::from_reader(Cursor::new(bytes), format)
        .map_err(|error| RpcError::internal(format!("无法解析 Office 文件: {error}")))?;
    let html = sanitize(&document.to_html());
    if html.trim().is_empty() {
        return Ok(json!({ "kind": extension, "html": "", "empty": true }));
    }
    Ok(json!({ "kind": extension, "html": html }))
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
}
