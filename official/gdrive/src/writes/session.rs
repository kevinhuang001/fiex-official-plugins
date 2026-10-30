use super::*;
pub(super) struct Started {
    pub id: String,
    pub url: String,
    pub revision: String,
    pub etag: String,
    pub creating: bool,
    pub content_type: String,
}
pub(super) fn start(
    request: &FsRequest<'_>,
    http: &sdk::HttpClient,
    token: &str,
    size: u64,
) -> Result<Started, RpcError> {
    let creating = request.node_id().is_none_or(str::is_empty);
    let (id, revision, etag, method, url, body, content_type) = if creating {
        let parent = request
            .parent_id()
            .filter(|id| !id.is_empty())
            .unwrap_or("root");
        node(parent)?;
        let name = request
            .name()
            .ok_or_else(|| RpcError::invalid_params("missing name"))?;
        if name.is_empty() || matches!(name, "." | "..") || name.contains(['/', '\\', '\0']) {
            return Err(RpcError::invalid_params("invalid name"));
        }
        let (folder, _) = metadata(http, token, parent)?;
        if folder["mimeType"] != "application/vnd.google-apps.folder"
            || folder["capabilities"]["canAddChildren"] != true
        {
            return Err(error("Drive folder does not permit child creation"));
        }
        (String::new(), String::new(), String::new(), "POST",
            format!("https://www.googleapis.com/upload/drive/v3/files?uploadType=resumable&fields={FILE_FIELDS}&supportsAllDrives=true"),
            json!({"name":name,"parents":[parent]}).to_string(),
            "application/octet-stream".to_string())
    } else {
        let id = node(
            request
                .node_id()
                .ok_or_else(|| RpcError::invalid_params("missing nodeId"))?,
        )?;
        let revision = request
            .if_revision()
            .ok_or_else(|| error("missing ifRevision"))?;
        let (file, etag) = metadata(http, token, id)?;
        if file["version"].as_str() != Some(revision) {
            return Err(error(crate::i18n::t("同步冲突：云端版本已改变")));
        }
        if etag.is_empty() {
            return Err(error(
                "Drive did not return an ETag; refusing unsafe overwrite",
            ));
        }
        if file["capabilities"]["canModifyContent"] != true {
            return Err(error("Drive file is read-only"));
        }
        // Native editors have no binary of their own: the cache holds the
        // exported Office representation, and this update tells Drive to
        // import it back into the same editor, replacing all content.
        let (body, content_type) =
            match reads::import_format(file["mimeType"].as_str().unwrap_or("")) {
                Some(import) => (
                    json!({"mimeType": import.target}).to_string(),
                    import.source.to_string(),
                ),
                None => ("{}".to_string(), "application/octet-stream".to_string()),
            };
        (id.to_owned(), revision.to_owned(), etag, "PATCH",
            format!("https://www.googleapis.com/upload/drive/v3/files/{id}?uploadType=resumable&fields={FILE_FIELDS}&supportsAllDrives=true"), body, content_type)
    };
    let mut upload = http_request(http, token, method, &url)
        .header("X-Upload-Content-Length", size.to_string())
        .header("X-Upload-Content-Type", &content_type)
        .header("Content-Type", "application/json");
    if !etag.is_empty() {
        upload = upload.header("If-Match", &etag);
    }
    let response = upload
        .body(body)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(error)?;
    let url = response
        .headers
        .get("location")
        .ok_or_else(|| error("missing session URL"))?
        .clone();
    if !url.starts_with("https://www.googleapis.com/upload/drive/") {
        return Err(error("invalid upload session host"));
    }
    Ok(Started {
        id,
        url,
        revision,
        etag,
        creating,
        content_type,
    })
}
