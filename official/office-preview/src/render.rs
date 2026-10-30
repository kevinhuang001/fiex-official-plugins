//! Format detection and rendering: parse the OOXML package with `office_oxide`
//! and sanitize the HTML it produces.
use crate::sanitize::sanitize;
use fiex_plugin_sdk::RpcError;
use office_oxide::ir::{DocumentIR, Element};
use office_oxide::{Document, DocumentFormat};
use serde_json::{json, Value};
use std::io::Cursor;

fn format(extension: &str) -> Result<DocumentFormat, RpcError> {
    match extension {
        "docx" => Ok(DocumentFormat::Docx),
        "xlsx" => Ok(DocumentFormat::Xlsx),
        "pptx" => Ok(DocumentFormat::Pptx),
        _ => Err(RpcError::invalid_params(crate::i18n::t(
            "仅支持 docx、xlsx、pptx",
        ))),
    }
}

pub(crate) fn extension(path: &str) -> String {
    path.rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default()
}

/// DOCX has no page-section boundary in the IR; a manual page break is the
/// only reliable "next page" marker. Keep the break that starts the cut page
/// out, so the last rendered page is complete.
fn limit_docx(ir: &mut DocumentIR, limit: usize) -> bool {
    let mut pages = 1usize;
    let mut keep_sections = 0usize;
    for section in &mut ir.sections {
        keep_sections += 1;
        let mut kept = Vec::new();
        let mut cut = false;
        for element in section.elements.drain(..) {
            let is_break = matches!(element, Element::PageBreak);
            kept.push(element);
            if is_break {
                if pages >= limit {
                    cut = true;
                    break;
                }
                pages += 1;
            }
        }
        section.elements = kept;
        if cut {
            ir.sections.truncate(keep_sections);
            return true;
        }
    }
    if ir.sections.len() > limit {
        ir.sections.truncate(limit);
        return true;
    }
    false
}

fn parse_limited(
    bytes: Vec<u8>,
    extension: &str,
    limit: usize,
) -> Result<(DocumentIR, DocumentFormat, usize, bool), RpcError> {
    let format = format(extension)?;
    let document = Document::from_reader(Cursor::new(bytes), format).map_err(|error| {
        RpcError::internal(fiex_plugin_sdk::i18n::format(
            crate::i18n::t("无法解析 Office 文件: {error}"),
            &[("error", format!("{}", error))],
        ))
    })?;
    let mut ir = document.to_ir();
    let total_sections = ir.sections.len();
    let limit = limit.max(1);
    let truncated = if format == DocumentFormat::Docx {
        limit_docx(&mut ir, limit)
    } else if total_sections > limit {
        ir.sections.truncate(limit);
        true
    } else {
        false
    };
    Ok((ir, format, total_sections, truncated))
}

pub(crate) fn render_pdf_pages(
    bytes: Vec<u8>,
    extension: &str,
    limit: usize,
) -> Result<Value, RpcError> {
    crate::pdf::images(&bytes, extension, limit)
}

pub(crate) fn render(bytes: Vec<u8>, extension: &str, limit: usize) -> Result<Value, RpcError> {
    let (ir, _format, total_sections, truncated) = parse_limited(bytes, extension, limit)?;
    let limit = limit.max(1);
    let html = sanitize(&ir.to_html());
    if html.trim().is_empty() {
        return Ok(json!({
            "kind": extension,
            "html": "",
            "empty": true,
            "pages": limit,
            "totalSections": total_sections,
            "truncated": false,
        }));
    }
    Ok(json!({
        "kind": extension,
        "html": html,
        "pages": limit,
        "totalSections": total_sections,
        "truncated": truncated,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(format!("tests/fixtures/{name}")).unwrap()
    }

    #[test]
    fn renders_docx_structure() {
        let value = render(fixture("fixture.docx"), "docx", 20).unwrap();
        let html = value["html"].as_str().unwrap();
        assert!(html.contains("<h1>"), "{html}");
        assert!(html.contains("2026 年第三季度产品报告"));
        assert!(html.contains("<table>"));
        assert!(html.contains("1,200,000"));
        assert!(html.contains("<ul>"));
    }

    #[test]
    fn renders_xlsx_sheets() {
        let value = render(fixture("fixture.xlsx"), "xlsx", 20).unwrap();
        let html = value["html"].as_str().unwrap();
        assert!(html.contains("预算"));
        assert!(html.contains("收入"));
        assert!(html.contains("<table>"));
        assert!(html.contains("1120000"));
    }

    #[test]
    fn renders_pptx_slides() {
        let value = render(fixture("fixture.pptx"), "pptx", 20).unwrap();
        let html = value["html"].as_str().unwrap();
        assert!(html.contains("Fiex 产品介绍"));
        assert!(html.contains("插件化"));
        assert!(html.contains("下一步"));
    }

    #[test]
    fn limits_pptx_to_the_configured_slide_count() {
        let full = render(fixture("fixture.pptx"), "pptx", 20).unwrap();
        assert!(full["html"].as_str().unwrap().contains("下一步"));
        let limited = render(fixture("fixture.pptx"), "pptx", 1).unwrap();
        let html = limited["html"].as_str().unwrap();
        assert!(!html.contains("下一步"), "{html}");
        assert_eq!(limited["truncated"], true);
        assert_eq!(limited["pages"], 1);
    }

    #[test]
    fn renders_bounded_pdf_pages_as_png() {
        use base64::Engine;
        let value = render_pdf_pages(fixture("fixture.docx"), "docx", 1).unwrap();
        let pages = value["pages"].as_array().unwrap();
        assert_eq!(pages.len(), 1);
        let png = base64::engine::general_purpose::STANDARD
            .decode(pages[0].as_str().unwrap())
            .unwrap();
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        let limited = render_pdf_pages(fixture("fixture.pptx"), "pptx", 1).unwrap();
        assert_eq!(limited["pages"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn rejects_unknown_and_corrupt_files() {
        assert!(render(Vec::new(), "docx", 6).is_err());
        assert!(render(Vec::new(), "doc", 6).is_err());
    }
}
