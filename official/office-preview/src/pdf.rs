//! Plugin-owned Office-to-PDF preview.
//!
//! `office2pdf` converts DOCX/PPTX into a real PDF using pure Rust. The plugin
//! then rasterizes only the requested first pages with `hayro` for its own
//! viewer. Core never parses the document or renders the PDF.
use base64::Engine;
use fiex_plugin_sdk::RpcError;
use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{render as render_page, PixmapSettings, RenderCache, RenderSettings};
use office2pdf::config::{ConvertOptions, Format, SlideRange};
use office2pdf::pdf_ops::{self, PageRange};

const MAX_PDF: usize = 32 * 1024 * 1024;
const MAX_ENCODED: usize = 2_500_000;

fn format(extension: &str) -> Result<Format, RpcError> {
    Format::from_extension(extension)
        .ok_or_else(|| RpcError::invalid_params("only docx and pptx can be converted to PDF"))
}

/// Convert the document and keep at most `limit` PDF pages.
fn convert(bytes: &[u8], extension: &str, limit: usize) -> Result<Vec<u8>, RpcError> {
    let format = format(extension)?;
    let limit = limit.clamp(1, 30) as u32;
    let mut options = ConvertOptions::default();
    if format == Format::Pptx {
        options.slide_range = Some(SlideRange::new(1, limit));
    }
    let result = office2pdf::convert_bytes(bytes, format, &options)
        .map_err(|error| RpcError::internal(format!("Office conversion failed: {error}")))?;
    let mut pdf = result.pdf;
    if format == Format::Docx {
        let pages = pdf_ops::page_count(&pdf)
            .map_err(|error| RpcError::internal(format!("cannot count PDF pages: {error}")))?;
        if pages > limit {
            let mut parts = pdf_ops::split(&pdf, &[PageRange::new(1, limit)])
                .map_err(|error| RpcError::internal(format!("cannot trim PDF pages: {error}")))?;
            pdf = parts
                .pop()
                .ok_or_else(|| RpcError::internal("cannot trim PDF pages"))?;
        }
    }
    if pdf.len() > MAX_PDF {
        return Err(RpcError::internal("generated PDF is too large"));
    }
    Ok(pdf)
}

/// Render the requested PDF pages to PNG images for the plugin-owned viewer.
pub(crate) fn images(
    bytes: &[u8],
    extension: &str,
    limit: usize,
) -> Result<serde_json::Value, RpcError> {
    let pdf_bytes = convert(bytes, extension, limit)?;
    let pdf = Pdf::new(pdf_bytes).map_err(|error| RpcError::internal(format!("{error:?}")))?;
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
