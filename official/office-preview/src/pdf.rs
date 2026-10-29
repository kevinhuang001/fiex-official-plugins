//! Pure-Rust Office IR → PDF rendering.
//!
//! Only the requested first pages are laid out and written. DOCX is cut at the
//! Nth page break before layout; PPTX only visits the first N slides; XLSX is
//! handled by the data-preview plugin. The font is bundled with the plugin so
//! Chinese text does not depend on a system font or an external converter.
use base64::Engine;
use fiex_plugin_sdk::RpcError;
use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{render as render_page, PixmapSettings, RenderCache, RenderSettings};
use office_oxide::ir::{Element, InlineContent, List, Section, Table};
use office_oxide::{DocumentFormat, DocumentIR};
use printpdf::*;

const FONT: &[u8] = include_bytes!("../assets/DroidSansFallbackFull.ttf");
const MAX_PDF: usize = 32 * 1024 * 1024;
const TWIP_MM: f32 = 25.4 / 1440.0;
const POINT_MM: f32 = 25.4 / 72.0;

fn text(content: &[InlineContent]) -> String {
    office_oxide::ir::inline_to_text(content)
}

fn wrap(value: &str, width_mm: f32, size_pt: f32) -> Vec<String> {
    let em = (size_pt * POINT_MM).max(1.0);
    let mut result = Vec::new();
    for paragraph in value.split('\n') {
        let paragraph = paragraph.replace('\t', "    ");
        let mut line = String::new();
        let mut used = 0.0f32;
        for ch in paragraph.chars() {
            let advance = if ch.is_ascii() { em * 0.52 } else { em };
            if !line.is_empty() && used + advance > width_mm {
                result.push(std::mem::take(&mut line));
                used = 0.0;
            }
            line.push(ch);
            used += advance;
        }
        result.push(line);
    }
    if result.is_empty() {
        result.push(String::new());
    }
    result
}

fn setup(section: &Section, format: DocumentFormat) -> (f32, f32, f32) {
    if let Some(page) = &section.page_setup {
        let width = page.width_twips as f32 * TWIP_MM;
        let height = page.height_twips as f32 * TWIP_MM;
        let left = page.margin_left_twips as f32 * TWIP_MM;
        let right = page.margin_right_twips as f32 * TWIP_MM;
        let top = page.margin_top_twips as f32 * TWIP_MM;
        let bottom = page.margin_bottom_twips as f32 * TWIP_MM;
        return (
            width,
            height,
            left.max(right).max(top).max(bottom).max(10.0),
        );
    }
    match format {
        DocumentFormat::Pptx => (338.7, 190.5, 12.0),
        _ => (210.0, 297.0, 18.0),
    }
}

struct Renderer {
    font: FontId,
    pages: Vec<PdfPage>,
    ops: Vec<Op>,
    width: f32,
    height: f32,
    margin: f32,
    y: f32,
    started: bool,
    limit: usize,
}

impl Renderer {
    fn new(font: FontId, limit: usize) -> Self {
        Self {
            font,
            pages: Vec::new(),
            ops: Vec::new(),
            width: 210.0,
            height: 297.0,
            margin: 18.0,
            y: 279.0,
            started: false,
            limit,
        }
    }

    fn set_size(&mut self, width: f32, height: f32, margin: f32) {
        self.width = width;
        self.height = height;
        self.margin = margin;
        self.y = height - margin;
    }

    fn flush(&mut self) -> bool {
        if self.started {
            self.pages.push(PdfPage::new(
                Mm(self.width),
                Mm(self.height),
                std::mem::take(&mut self.ops),
            ));
            self.started = false;
        }
        self.pages.len() < self.limit
    }

    fn new_page(&mut self) -> bool {
        if !self.flush() {
            return false;
        }
        self.started = true;
        self.ops.clear();
        self.y = self.height - self.margin;
        true
    }

    fn ensure(&mut self, space: f32) -> bool {
        if !self.started && !self.new_page() {
            return false;
        }
        if self.y - space < self.margin {
            if !self.new_page() {
                return false;
            }
        }
        true
    }

    fn line(&mut self, value: &str, size: f32, indent: f32, color: [f32; 3]) {
        let width = (self.width - 2.0 * self.margin - indent).max(10.0);
        for line in wrap(value, width, size) {
            let leading = (size * 1.45 * POINT_MM).max(3.0);
            if !self.ensure(leading) {
                return;
            }
            self.y -= leading;
            self.ops.extend([
                Op::StartTextSection,
                Op::SetTextCursor {
                    pos: Point::new(Mm(self.margin + indent), Mm(self.y.max(self.margin))),
                },
                Op::SetFont {
                    font: PdfFontHandle::External(self.font.clone()),
                    size: Pt(size),
                },
                Op::SetLineHeight {
                    lh: Pt(size * 1.45),
                },
                Op::SetFillColor {
                    col: Color::Rgb(Rgb {
                        r: color[0],
                        g: color[1],
                        b: color[2],
                        icc_profile: None,
                    }),
                },
                Op::ShowText {
                    items: vec![TextItem::Text(line)],
                },
                Op::EndTextSection,
            ]);
        }
    }

    fn gap(&mut self, mm: f32) {
        if self.started {
            self.y -= mm;
        }
    }

    fn block(&mut self, elements: &[Element], indent: f32, depth: usize) {
        if depth > 32 {
            return;
        }
        for element in elements {
            match element {
                Element::Heading(heading) => {
                    let size = 20.0 - heading.clamped_level() as f32 * 2.0;
                    self.gap(2.0);
                    self.line(
                        &text(&heading.content),
                        size.max(11.0),
                        indent,
                        [0.08, 0.1, 0.14],
                    );
                    self.gap(1.5);
                }
                Element::Paragraph(paragraph) => {
                    let size = office_oxide::ir::first_inline_font_size_pt(&paragraph.content)
                        .unwrap_or(11.0)
                        .clamp(7.0, 20.0);
                    self.line(&text(&paragraph.content), size, indent, [0.12, 0.14, 0.18]);
                }
                Element::List(list) => self.list(list, indent, depth),
                Element::Table(table) => self.table(table, indent),
                Element::Image(_) => {
                    self.line(crate::i18n::t("[图片]"), 10.0, indent, [0.45, 0.45, 0.45])
                }
                Element::ThematicBreak => {
                    self.line("────────────────", 10.0, indent, [0.6, 0.6, 0.6])
                }
                Element::PageBreak => {
                    self.new_page();
                }
                Element::TextBox(text_box) => self.block(&text_box.content, indent, depth + 1),
                Element::Footnote(note) | Element::Endnote(note) => {
                    self.block(&note.content, indent, depth + 1)
                }
                Element::CodeBlock(code) => self.line(&code.content, 9.0, indent, [0.2, 0.2, 0.2]),
                Element::ColumnBreak | Element::Shape(_) => {}
                _ => {}
            }
        }
    }

    fn list(&mut self, list: &List, indent: f32, depth: usize) {
        let mut number = list.start_number.unwrap_or(1);
        for item in &list.items {
            let marker = if list.ordered {
                let value = format!("{number}. ");
                number += 1;
                value
            } else {
                "• ".to_string()
            };
            self.line(&marker, 11.0, indent, [0.12, 0.14, 0.18]);
            self.block(&item.content, indent + 5.0, depth + 1);
            if let Some(nested) = &item.nested {
                self.list(nested, indent + 8.0, depth + 1);
            }
        }
    }

    fn table(&mut self, table: &Table, indent: f32) {
        for row in &table.rows {
            let cells = row
                .cells
                .iter()
                .map(|cell| {
                    cell.content
                        .iter()
                        .map(|element| match element {
                            Element::Paragraph(paragraph) => text(&paragraph.content),
                            Element::Heading(heading) => text(&heading.content),
                            _ => String::new(),
                        })
                        .filter(|value| !value.is_empty())
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .collect::<Vec<_>>();
            let size = if row.is_header { 9.5 } else { 9.0 };
            self.line(&cells.join("  |  "), size, indent, [0.12, 0.14, 0.18]);
        }
        self.gap(1.0);
    }
}

/// Render the already page-limited IR to PDF bytes.
pub(crate) fn render(
    ir: &DocumentIR,
    format: DocumentFormat,
    limit: usize,
) -> Result<Vec<u8>, RpcError> {
    let limit = limit.clamp(1, 30);
    let mut doc = PdfDocument::new("Fiex Office Preview");
    let parsed = ParsedFont::from_bytes(FONT, 0, &mut Vec::new())
        .ok_or_else(|| RpcError::internal("bundled PDF font is invalid"))?;
    let font = doc.add_font(&parsed);
    let mut renderer = Renderer::new(font, limit);
    for section in &ir.sections {
        if renderer.pages.len() >= limit {
            break;
        }
        let (width, height, margin) = setup(section, format);
        let starts_page = format == DocumentFormat::Pptx
            || renderer.pages.is_empty()
            || !matches!(
                section.break_type,
                office_oxide::ir::SectionBreakType::Continuous
            );
        if starts_page && (renderer.started || !renderer.pages.is_empty()) {
            if !renderer.flush() {
                break;
            }
        }
        renderer.set_size(width, height, margin);
        if let Some(title) = &section.title {
            if format == DocumentFormat::Pptx {
                renderer.line(title, 22.0, 0.0, [0.06, 0.08, 0.12]);
                renderer.gap(2.0);
            }
        }
        renderer.block(&section.elements, 0.0, 0);
    }
    if !renderer.flush() && renderer.pages.is_empty() {
        renderer.pages.push(PdfPage::new(
            Mm(renderer.width),
            Mm(renderer.height),
            Vec::new(),
        ));
    }
    if renderer.pages.len() > limit {
        renderer.pages.truncate(limit);
    }
    let mut options = PdfSaveOptions::default();
    options.subset_fonts = true;
    options.optimize = false;
    let mut warnings = Vec::new();
    let bytes = doc.with_pages(renderer.pages).save(&options, &mut warnings);
    if bytes.len() > MAX_PDF {
        return Err(RpcError::internal("generated PDF is too large"));
    }
    Ok(bytes)
}

/// Render the requested pages to PNG images for the plugin-owned viewer.
/// Core only hosts the plugin HTML; it never sees or renders the PDF.
pub(crate) fn images(
    ir: &DocumentIR,
    format: DocumentFormat,
    limit: usize,
) -> Result<serde_json::Value, RpcError> {
    const MAX_ENCODED: usize = 2_500_000;
    let bytes = render(ir, format, limit)?;
    let pdf = Pdf::new(bytes).map_err(|error| RpcError::internal(format!("{error:?}")))?;
    let cache = RenderCache::new();
    let settings = InterpreterSettings::default();
    let render_settings = RenderSettings::default();
    let pixmap_settings = PixmapSettings {
        x_scale: 1.0,
        y_scale: 1.0,
        bg_color: WHITE,
    };
    let mut pages = Vec::new();
    let mut encoded_bytes = 0usize;
    for (index, page) in pdf.pages().iter().enumerate() {
        if index >= limit {
            break;
        }
        let pixmap = render_page(page, &cache, &settings, &render_settings, &pixmap_settings);
        let png = pixmap
            .into_png()
            .map_err(|error| RpcError::internal(error.to_string()))?;
        encoded_bytes = encoded_bytes.saturating_add(png.len());
        if encoded_bytes > MAX_ENCODED {
            return Err(RpcError::internal("rendered PDF pages are too large"));
        }
        pages.push(base64::engine::general_purpose::STANDARD.encode(png));
    }
    Ok(serde_json::json!({
        "pages": pages,
        "count": pages.len(),
    }))
}
