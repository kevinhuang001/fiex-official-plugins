//! Plugin-owned Office preview rendering.
//!
//! `office_oxide` parses the document into its HTML representation, `printpdf`'s
//! HTML layout engine turns that into PDF pages, and `hayro` rasterizes only the
//! requested first pages for the plugin viewer. Core never sees the document or
//! the PDF bytes.
use base64::Engine;
use fiex_plugin_sdk::RpcError;
use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{render as render_page, PixmapSettings, RenderCache, RenderSettings};
use office_oxide::ir::DocumentIR;
use office_oxide::DocumentFormat;
use printpdf::*;
use std::collections::BTreeMap;

const FONT: &[u8] = include_bytes!("../assets/NotoSansSC-Subset.otf");
const MAX_PDF: usize = 32 * 1024 * 1024;

fn page_size(ir: &DocumentIR, format: DocumentFormat) -> (f32, f32) {
    if let Some(page) = ir
        .sections
        .first()
        .and_then(|section| section.page_setup.as_ref())
    {
        return (
            page.width_twips as f32 * 25.4 / 1440.0,
            page.height_twips as f32 * 25.4 / 1440.0,
        );
    }
    if format == DocumentFormat::Pptx {
        (338.7, 190.5)
    } else {
        (210.0, 297.0)
    }
}

fn html(ir: &DocumentIR) -> String {
    format!(
        "<style>
         html,body{{margin:0;padding:0;font-family:'Noto Sans SC',sans-serif;font-size:11pt;line-height:1.5;color:#1d2430}}
         h1{{font-size:20pt;line-height:1.25;margin:14pt 0 8pt}}
         h2{{font-size:15pt;line-height:1.3;margin:13pt 0 7pt}}
         h3{{font-size:13pt;line-height:1.35;margin:11pt 0 6pt}}
         p{{margin:0 0 7pt}}
         ul,ol{{margin:0 0 8pt;padding-left:20pt}}
         li{{margin:2pt 0}}
         table{{border-collapse:collapse;width:100%;margin:6pt 0 10pt;font-size:9.5pt}}
         th,td{{border:1px solid #c8d0da;padding:4pt 6pt;text-align:left;vertical-align:top}}
         th{{background:#f1f4f8;font-weight:600}}
         img{{max-width:100%;height:auto}}
         hr{{border:0;border-top:1px solid #c8d0da;margin:12pt 0}}
         </style>{}",
        ir.to_html()
    )
}

/// Generate a real PDF document containing at most `limit` pages.
pub(crate) fn render(
    ir: &DocumentIR,
    format: DocumentFormat,
    limit: usize,
) -> Result<Vec<u8>, RpcError> {
    let limit = limit.clamp(1, 30);
    let (width, height) = page_size(ir, format);
    let mut fonts = BTreeMap::new();
    fonts.insert("Noto Sans SC".to_string(), Base64OrRaw::Raw(FONT.to_vec()));
    let options = GeneratePdfOptions {
        page_width: Some(width),
        page_height: Some(height),
        margin_top: Some(14.0),
        margin_bottom: Some(14.0),
        margin_left: Some(16.0),
        margin_right: Some(16.0),
        font_embedding: Some(true),
        ..Default::default()
    };
    let mut warnings = Vec::new();
    let mut document =
        PdfDocument::from_html(&html(ir), &BTreeMap::new(), &fonts, &options, &mut warnings)
            .map_err(|error| {
                RpcError::internal(format!("cannot lay out Office preview: {error}"))
            })?;
    if document.pages.is_empty() {
        return Err(RpcError::internal("Office preview produced no pages"));
    }
    if document.pages.len() > limit {
        document.pages.truncate(limit);
    }
    let mut save_warnings = Vec::new();
    let bytes = document.save(&PdfSaveOptions::default(), &mut save_warnings);
    if bytes.len() > MAX_PDF {
        return Err(RpcError::internal("generated PDF is too large"));
    }
    Ok(bytes)
}

/// Render the requested pages to PNG images for the plugin-owned viewer.
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
        x_scale: 1.2,
        y_scale: 1.2,
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
