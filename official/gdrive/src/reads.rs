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
        let export = export_mime(mime)
            .ok_or_else(|| error("this Google Drive item cannot be exported for reading"))?;
        let response = http_request(
            &ctx.http(),
            &token,
            "GET",
            &format!(
                "{API}/files/{id}/export?mimeType={}",
                urlencoding::encode(export)
            ),
        )
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(error)?;
        return Ok(FsRead::new(response, None));
    }
    let size = file["size"].as_str().and_then(|s| s.parse::<u64>().ok());
    // The SDK owns chunk offsets and seek-forward; a plain response body is the
    // read stream. Core never sees Drive paths, only the file ID.
    let response = http_request(
        &ctx.http(),
        &token,
        "GET",
        &format!("{API}/files/{id}?alt=media&supportsAllDrives=true"),
    )
    .send()
    .and_then(|r| r.error_for_status())
    .map_err(error)?;
    Ok(FsRead::new(response, size))
}

pub(super) fn export_mime(mime: &str) -> Option<&'static str> {
    match mime {
        "application/vnd.google-apps.document"
        | "application/vnd.google-apps.spreadsheet"
        | "application/vnd.google-apps.presentation"
        | "application/vnd.google-apps.drawing"
        | "application/vnd.google-apps.form" => Some("application/pdf"),
        "application/vnd.google-apps.script" => Some("application/vnd.google-apps.script+json"),
        _ => None,
    }
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
    fn native_editor_exports_are_readable() {
        assert_eq!(
            export_mime("application/vnd.google-apps.document"),
            Some("application/pdf")
        );
        assert_eq!(
            export_mime("application/vnd.google-apps.spreadsheet"),
            Some("application/pdf")
        );
        assert_eq!(
            export_mime("application/vnd.google-apps.presentation"),
            Some("application/pdf")
        );
        assert_eq!(
            export_mime("application/vnd.google-apps.script"),
            Some("application/vnd.google-apps.script+json")
        );
        assert_eq!(export_mime("application/vnd.google-apps.shortcut"), None);
    }

    #[test]
    fn safe_unique_names_preserve_extensions() {
        assert_eq!(safe_name("a/b.txt", "abc"), "a_b [abc].txt");
        assert_ne!(safe_name("same", "a"), safe_name("same", "b"));
    }
}
