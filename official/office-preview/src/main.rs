//! Read-only Word / Excel / PowerPoint preview.
//!
//! This is a text-level renderer: paragraphs, table cells and slide text are
//! extracted from the OOXML parts. Charts, SmartArt, animations, embedded
//! objects and legacy `.doc/.xls/.ppt` are explicitly out of scope.
use fiex_plugin_sdk::{self as sdk, RpcError};
use quick_xml::events::Event;
use quick_xml::Reader;
use serde_json::{json, Value};
use std::io::{Cursor, Read};
use zip::ZipArchive;

const MAX_ROWS: usize = 2000;
const MAX_COLUMNS: usize = 64;

fn xml_part(archive: &mut ZipArchive<Cursor<Vec<u8>>>, name: &str) -> Option<String> {
    let mut file = archive.by_name(name).ok()?;
    let mut text = String::new();
    file.read_to_string(&mut text).ok()?;
    Some(text)
}

fn local_name(name: quick_xml::name::QName<'_>) -> Vec<u8> {
    let raw = name.as_ref();
    raw.rsplit(|byte| *byte == b':')
        .next()
        .unwrap_or(raw)
        .to_vec()
}

fn docx_lines(xml: &str) -> Vec<String> {
    let mut reader = Reader::from_str(xml);
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut in_text = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) if local_name(event.name()) == b"t" => in_text = true,
            Ok(Event::End(event)) if local_name(event.name()) == b"t" => in_text = false,
            Ok(Event::End(event)) if local_name(event.name()) == b"p" => {
                let line = current.trim().to_string();
                if !line.is_empty() {
                    lines.push(line);
                }
                current.clear();
            }
            Ok(Event::Text(text)) if in_text => {
                current.push_str(&text.unescape().unwrap_or_default())
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    lines.truncate(MAX_ROWS);
    lines
}

fn shared_strings(xml: &str) -> Vec<String> {
    let mut reader = Reader::from_str(xml);
    let mut values = Vec::new();
    let mut current = String::new();
    let mut in_item = false;
    let mut in_text = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) if local_name(event.name()) == b"si" => {
                in_item = true;
                current.clear();
            }
            Ok(Event::Start(event)) if in_item && local_name(event.name()) == b"t" => {
                in_text = true
            }
            Ok(Event::End(event)) if in_item && local_name(event.name()) == b"t" => in_text = false,
            Ok(Event::End(event)) if local_name(event.name()) == b"si" => {
                values.push(current.clone());
                in_item = false;
            }
            Ok(Event::Text(text)) if in_text => {
                current.push_str(&text.unescape().unwrap_or_default())
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    values
}

fn local_name_key(key: &[u8]) -> &[u8] {
    key.rsplit(|byte| *byte == b':').next().unwrap_or(key)
}

fn column_index(reference: &str) -> Option<usize> {
    let letters = reference
        .bytes()
        .take_while(|byte| byte.is_ascii_uppercase())
        .collect::<Vec<_>>();
    if letters.is_empty() {
        return None;
    }
    let mut value = 0usize;
    for letter in letters {
        value = value * 26 + (letter - b'A' + 1) as usize;
    }
    Some(value - 1)
}

fn sheet_rows(xml: &str, strings: &[String]) -> Vec<Vec<String>> {
    let mut reader = Reader::from_str(xml);
    let mut rows = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut cell_column = 0usize;
    let mut cell_type = String::new();
    let mut value = String::new();
    let mut in_value = false;
    let mut in_inline = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) if local_name(event.name()) == b"c" => {
                cell_column = event
                    .attributes()
                    .filter_map(|attribute| attribute.ok())
                    .find(|attribute| local_name_key(attribute.key.as_ref()) == b"r")
                    .and_then(|attribute| attribute.unescape_value().ok())
                    .and_then(|value| column_index(&value))
                    .unwrap_or(cell_column);
                cell_type = event
                    .attributes()
                    .filter_map(|attribute| attribute.ok())
                    .find(|attribute| local_name_key(attribute.key.as_ref()) == b"t")
                    .and_then(|attribute| attribute.unescape_value().ok())
                    .unwrap_or_default()
                    .to_string();
                value.clear();
            }
            Ok(Event::Start(event)) if local_name(event.name()) == b"v" => in_value = true,
            Ok(Event::Start(event))
                if local_name(event.name()) == b"t" && cell_type == "inlineStr" =>
            {
                in_inline = true
            }
            Ok(Event::End(event)) if local_name(event.name()) == b"v" => in_value = false,
            Ok(Event::End(event)) if local_name(event.name()) == b"t" => in_inline = false,
            Ok(Event::End(event)) if local_name(event.name()) == b"c" => {
                if cell_column < MAX_COLUMNS {
                    while row.len() <= cell_column {
                        row.push(String::new());
                    }
                    let text = if cell_type == "s" {
                        value
                            .trim()
                            .parse::<usize>()
                            .ok()
                            .and_then(|index| strings.get(index).cloned())
                            .unwrap_or_default()
                    } else {
                        value.clone()
                    };
                    row[cell_column] = text;
                }
            }
            Ok(Event::End(event)) if local_name(event.name()) == b"row" => {
                let cells = row.drain(..).take(MAX_COLUMNS).collect::<Vec<_>>();
                if cells.iter().any(|cell| !cell.is_empty()) && rows.len() < MAX_ROWS {
                    rows.push(cells);
                }
                cell_column = 0;
            }
            Ok(Event::Text(text)) if in_value || in_inline => {
                value.push_str(&text.unescape().unwrap_or_default())
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    rows
}

fn pptx_lines(xml: &str) -> Vec<String> {
    let mut reader = Reader::from_str(xml);
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut in_text = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) if local_name(event.name()) == b"t" => in_text = true,
            // A paragraph break separates text boxes on the slide.
            Ok(Event::End(event)) if local_name(event.name()) == b"p" => {
                let line = current.trim().to_string();
                if !line.is_empty() {
                    lines.push(line);
                }
                current.clear();
            }
            Ok(Event::End(event)) if local_name(event.name()) == b"t" => in_text = false,
            Ok(Event::Text(text)) if in_text => {
                current.push_str(&text.unescape().unwrap_or_default())
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    lines
}

fn extension(path: &str) -> String {
    path.rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default()
}

fn render(bytes: Vec<u8>, format: &str) -> Result<Value, RpcError> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| RpcError::internal(format!("不是有效的 Office 文件: {error}")))?;
    match format {
        "docx" => {
            let xml = xml_part(&mut archive, "word/document.xml")
                .ok_or_else(|| RpcError::internal("缺少 word/document.xml"))?;
            Ok(json!({ "kind": "docx", "lines": docx_lines(&xml) }))
        }
        "xlsx" => {
            let strings = xml_part(&mut archive, "xl/sharedStrings.xml")
                .map(|xml| shared_strings(&xml))
                .unwrap_or_default();
            let names = (0..archive.len())
                .filter_map(|index| {
                    archive
                        .by_index(index)
                        .ok()
                        .map(|file| file.name().to_string())
                })
                .filter(|name| name.starts_with("xl/worksheets/") && name.ends_with(".xml"))
                .collect::<Vec<_>>();
            let mut sheets = Vec::new();
            for (index, name) in names.iter().enumerate() {
                let Some(xml) = xml_part(&mut archive, name) else {
                    continue;
                };
                sheets.push(json!({
                    "name": format!("Sheet{}", index + 1),
                    "rows": sheet_rows(&xml, &strings)
                }));
            }
            Ok(json!({ "kind": "xlsx", "sheets": sheets }))
        }
        "pptx" => {
            let mut names = (0..archive.len())
                .filter_map(|index| {
                    archive
                        .by_index(index)
                        .ok()
                        .map(|file| file.name().to_string())
                })
                .filter(|name| name.starts_with("ppt/slides/slide") && name.ends_with(".xml"))
                .collect::<Vec<_>>();
            names.sort();
            let mut slides = Vec::new();
            for (index, name) in names.iter().enumerate() {
                let Some(xml) = xml_part(&mut archive, name) else {
                    continue;
                };
                slides.push(json!({ "index": index + 1, "lines": pptx_lines(&xml) }));
            }
            Ok(json!({ "kind": "pptx", "slides": slides }))
        }
        _ => Err(RpcError::invalid_params("仅支持 docx、xlsx、pptx")),
    }
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
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn package(parts: &[(&str, &str)]) -> Vec<u8> {
        let mut buffer = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut buffer);
            for (name, body) in parts {
                writer
                    .start_file(*name, SimpleFileOptions::default())
                    .unwrap();
                writer.write_all(body.as_bytes()).unwrap();
            }
            writer.finish().unwrap();
        }
        buffer.into_inner()
    }

    const DOCUMENT: &str = r#"<?xml version="1.0"?><w:document xmlns:w="w"><w:body>
<w:p><w:r><w:t>第一段</w:t></w:r></w:p>
<w:p><w:r><w:t>第二段</w:t></w:r></w:p>
</w:body></w:document>"#;

    #[test]
    fn extracts_docx_paragraphs() {
        let value = render(package(&[("word/document.xml", DOCUMENT)]), "docx").unwrap();
        assert_eq!(value["kind"], "docx");
        assert_eq!(value["lines"][0], "第一段");
        assert_eq!(value["lines"][1], "第二段");
    }

    #[test]
    fn extracts_xlsx_cells_with_shared_strings() {
        let strings = r#"<?xml version="1.0"?><sst><si><t>名称</t></si><si><t>数值</t></si></sst>"#;
        let sheet = r#"<?xml version="1.0"?><worksheet><sheetData>
<row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c></row>
<row r="2"><c r="A2"><v>42</v></c></row>
</sheetData></worksheet>"#;
        let value = render(
            package(&[
                ("xl/sharedStrings.xml", strings),
                ("xl/worksheets/sheet1.xml", sheet),
            ]),
            "xlsx",
        )
        .unwrap();
        assert_eq!(value["sheets"][0]["rows"][0][0], "名称");
        assert_eq!(value["sheets"][0]["rows"][1][0], "42");
    }

    #[test]
    fn extracts_pptx_slide_text() {
        let slide = r#"<?xml version="1.0"?><p:sld><a:p><a:r><a:t>标题</a:t></a:r></a:p>
<a:p><a:r><a:t>正文</a:t></a:r></a:p></p:sld>"#;
        let value = render(package(&[("ppt/slides/slide1.xml", slide)]), "pptx").unwrap();
        assert_eq!(value["slides"][0]["lines"][0], "标题");
        assert_eq!(value["slides"][0]["lines"][1], "正文");
    }

    #[test]
    fn rejects_unknown_formats() {
        assert!(render(package(&[]), "doc").is_err());
        assert!(render(Vec::new(), "docx").is_err());
    }
}
