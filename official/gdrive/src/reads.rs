use super::*;
use sdk::filesystem::{FsEntry, FsListing, FsRead, FsRequest};

pub(super) fn list(request: &FsRequest<'_>) -> Result<FsListing, RpcError> {
    let ctx = request.ctx();
    let cfg = request
        .connection()
        .ok_or_else(|| RpcError::invalid_params("missing connection"))?;
    let token = token(&ctx.http(), cfg)?;
    // An empty node id is the mount root.
    let id = request
        .node_id()
        .filter(|value| !value.is_empty())
        .unwrap_or("root");
    node(id)?;
    let query = format!("'{id}' in parents and trashed = false");
    let cursor = request.cursor().filter(|s| *s != "0").unwrap_or("");
    let limit = request
        .params()
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(500)
        .clamp(1, 500);
    let url = format!("{API}/files?q={}&fields=nextPageToken,files({FILE_FIELDS})&pageSize={limit}&pageToken={}&supportsAllDrives=true&includeItemsFromAllDrives=true",
        urlencoding::encode(&query), urlencoding::encode(cursor));
    let page: Value = http_request(&ctx.http(), &token, "GET", &url)
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.json())
        .map_err(error)?;
    let entries = page["files"]
        .as_array()
        .ok_or_else(|| error("missing files"))?
        .iter()
        .map(entry)
        .collect::<Vec<_>>();
    Ok(FsListing {
        entries,
        next_cursor: page["nextPageToken"].as_str().map(str::to_owned),
        warnings: Vec::new(),
        truncated: false,
    })
}

pub(super) fn stat(request: &FsRequest<'_>) -> Result<FsEntry, RpcError> {
    let ctx = request.ctx();
    let cfg = request
        .connection()
        .ok_or_else(|| RpcError::invalid_params("missing connection"))?;
    let token = token(&ctx.http(), cfg)?;
    let id = request
        .node_id()
        .filter(|value| !value.is_empty())
        .unwrap_or("root");
    let (file, _) = metadata(&ctx.http(), &token, id)?;
    Ok(entry(&file))
}

pub(super) fn read(request: &FsRequest<'_>) -> Result<FsRead, RpcError> {
    let ctx = request.ctx();
    let cfg = request
        .connection()
        .ok_or_else(|| RpcError::invalid_params("missing connection"))?;
    let token = token(&ctx.http(), cfg)?;
    let id = node(
        request
            .node_id()
            .ok_or_else(|| RpcError::invalid_params("missing nodeId"))?,
    )?;
    let (file, _) = metadata(&ctx.http(), &token, id)?;
    if let Some(mime) = file["mimeType"]
        .as_str()
        .filter(|value| value.starts_with("application/vnd.google-apps."))
    {
        // Google editors have no binary stream. Export them to a format the
        // built-in preview and download paths can read; the editor document is
        // never modified.
        let export = export_format(mime)
            .ok_or_else(|| error("this Google Drive item cannot be exported for reading"))?
            .mime;
        let response = http_request(
            &ctx.http(),
            &token,
            "GET",
            &format!(
                "{API}/files/{id}/export?mimeType={}",
                urlencoding::encode(export)
            ),
        )
        .timeout(std::time::Duration::from_secs(3600))
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(error)?;
        let size = response
            .headers
            .get("content-length")
            .and_then(|value| value.trim().parse::<u64>().ok());
        if let Some(size) = size {
            return Ok(FsRead::new(response, Some(size)));
        }
        // Some export responses are chunked. Buffer them so Core receives an
        // exact length instead of treating the file as empty.
        let bytes = response.bytes().map_err(error)?;
        return Ok(FsRead::from_bytes(bytes));
    }
    let size = file["size"].as_str().and_then(|s| s.parse::<u64>().ok());
    // The SDK owns chunk offsets and seek-forward; a plain response body is the
    // read stream. Core never sees Drive paths, only the file ID.
    let media = format!("{API}/files/{id}?alt=media&supportsAllDrives=true");
    // Segmented downloads first stage into a host-owned temporary file.
    // For ordinary documents and previews, direct HTTP streaming avoids the
    // aria2 startup cost and the extra disk read/write cycle.
    if let Some(size) = size.filter(|size| *size >= 32 * 1024 * 1024) {
        if let Ok(reader) = ctx
            .aria2()
            .get(&media)
            .bearer_auth(&token)
            .size(size)
            .timeout(std::time::Duration::from_secs(3600))
            .send()
        {
            return Ok(FsRead::new(reader, Some(size)));
        }
    }
    let response = http_request(&ctx.http(), &token, "GET", &media)
        .timeout(std::time::Duration::from_secs(3600))
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(error)?;
    Ok(FsRead::new(response, size))
}

pub(super) struct ExportFormat {
    pub mime: &'static str,
    pub extension: &'static str,
}

/// Native editors have no binary of their own. Match each editor to its
/// natural downloadable format so a Doc becomes DOCX, not a lossy PDF.
pub(super) fn export_format(mime: &str) -> Option<ExportFormat> {
    let (mime, extension) = match mime {
        "application/vnd.google-apps.document" => (
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            "docx",
        ),
        "application/vnd.google-apps.spreadsheet" => (
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            "xlsx",
        ),
        "application/vnd.google-apps.presentation" => (
            "application/vnd.openxmlformats-officedocument.presentationml.presentation",
            "pptx",
        ),
        "application/vnd.google-apps.drawing" => ("image/png", "png"),
        "application/vnd.google-apps.script" => ("application/vnd.google-apps.script+json", "json"),
        _ => return None,
    };
    Some(ExportFormat { mime, extension })
}

/// Native editors that can be replaced through an upload+convert update.
/// Drawings and Apps Script have no reliable reverse import, so they stay
/// read-only even when Google allows editing them in the browser.
pub(super) struct ImportFormat {
    pub target: &'static str,
    pub source: &'static str,
}

pub(super) fn import_format(mime: &str) -> Option<ImportFormat> {
    let target = match mime {
        "application/vnd.google-apps.document" => "application/vnd.google-apps.document",
        "application/vnd.google-apps.spreadsheet" => "application/vnd.google-apps.spreadsheet",
        "application/vnd.google-apps.presentation" => "application/vnd.google-apps.presentation",
        _ => return None,
    };
    Some(ImportFormat {
        target,
        source: export_format(mime)?.mime,
    })
}

/// Drive permits duplicate names and slash characters. Include the stable ID
/// to make every logical mount path safe and unambiguous.
pub(super) fn safe_name(name: &str, id: &str) -> String {
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_control() || "/\\:*?\"<>|".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let safe = safe.trim_end_matches([' ', '.']);
    let (stem, extension) = safe.rsplit_once('.').unwrap_or((safe, ""));
    if extension.is_empty() {
        format!("{stem} [{id}]")
    } else {
        format!("{stem} [{id}].{extension}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_editors_export_to_their_natural_format() {
        assert_eq!(
            export_format("application/vnd.google-apps.document")
                .map(|format| (format.mime, format.extension)),
            Some((
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                "docx"
            ))
        );
        assert_eq!(
            export_format("application/vnd.google-apps.spreadsheet")
                .map(|format| (format.mime, format.extension)),
            Some((
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                "xlsx"
            ))
        );
        assert_eq!(
            export_format("application/vnd.google-apps.presentation")
                .map(|format| (format.mime, format.extension)),
            Some((
                "application/vnd.openxmlformats-officedocument.presentationml.presentation",
                "pptx"
            ))
        );
        assert_eq!(
            export_format("application/vnd.google-apps.drawing")
                .map(|format| (format.mime, format.extension)),
            Some(("image/png", "png"))
        );
        assert_eq!(
            export_format("application/vnd.google-apps.script")
                .map(|format| (format.mime, format.extension)),
            Some(("application/vnd.google-apps.script+json", "json"))
        );
        assert!(export_format("application/vnd.google-apps.shortcut").is_none());
    }

    #[test]
    fn safe_unique_names_preserve_extensions() {
        assert_eq!(safe_name("a/b.txt", "abc"), "a_b [abc].txt");
        assert_ne!(safe_name("same", "a"), safe_name("same", "b"));
    }
}
