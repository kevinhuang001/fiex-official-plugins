use super::*;
use sdk::filesystem::{FsEntry, FsRequest};
use serde_json::{json, Map};

const FOLDER: &str = "application/vnd.google-apps.folder";
const FIELDS: &str = "id,name,mimeType,size,version,trashed,capabilities(canEdit)";

fn valid_name(name: &str) -> Result<(), RpcError> {
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\', ':', '\0']) {
        return Err(RpcError::invalid_params("invalid name"));
    }
    Ok(())
}

pub(super) fn mkdir(request: &FsRequest<'_>) -> Result<FsEntry, RpcError> {
    let ctx = request.ctx();
    let cfg = request
        .connection()
        .ok_or_else(|| RpcError::invalid_params("missing connection"))?;
    let token = token(&ctx.http(), cfg)?;
    let parent = request.parent_id().unwrap_or("root");
    node(parent)?;
    let name = request
        .name()
        .ok_or_else(|| RpcError::invalid_params("missing name"))?;
    valid_name(name)?;
    let body = json!({"name": name, "mimeType": FOLDER, "parents": [parent]});
    let file: Value = http_request(
        &ctx.http(),
        &token,
        "POST",
        &format!("{API}/files?fields={FIELDS}&supportsAllDrives=true"),
    )
    .header("Content-Type", "application/json")
    .body(body.to_string())
    .send()
    .and_then(|r| r.error_for_status())
    .and_then(|r| r.json())
    .map_err(error)?;
    Ok(entry(&file))
}

pub(super) fn rename(request: &FsRequest<'_>) -> Result<FsEntry, RpcError> {
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
    if let Some(revision) = request.if_revision() {
        if file["version"].as_str() != Some(revision) {
            return Err(error("同步冲突：云端版本已改变"));
        }
    }
    let mut body = Map::new();
    if let Some(name) = request.name() {
        valid_name(name)?;
        body.insert("name".into(), json!(name));
    }
    let mut query = String::new();
    if let Some(parent) = request.parent_id() {
        node(parent)?;
        let parents: Value = http_request(
            &ctx.http(),
            &token,
            "GET",
            &format!("{API}/files/{id}?fields=parents&supportsAllDrives=true"),
        )
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.json())
        .map_err(error)?;
        let current = parents["parents"]
            .as_array()
            .and_then(|values| values.first())
            .and_then(Value::as_str)
            .unwrap_or("");
        if current != parent {
            query.push_str(&format!("&addParents={}", urlencoding::encode(parent)));
            if !current.is_empty() {
                query.push_str(&format!("&removeParents={}", urlencoding::encode(current)));
            }
        }
    }
    if body.is_empty() && query.is_empty() {
        return Ok(entry(&file));
    }
    let file: Value = http_request(
        &ctx.http(),
        &token,
        "PATCH",
        &format!("{API}/files/{id}?fields={FIELDS}&supportsAllDrives=true{query}"),
    )
    .header("Content-Type", "application/json")
    .body(Value::Object(body).to_string())
    .send()
    .and_then(|r| r.error_for_status())
    .and_then(|r| r.json())
    .map_err(error)?;
    Ok(entry(&file))
}

pub(super) fn remove(request: &FsRequest<'_>) -> Result<(), RpcError> {
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
    if let Some(revision) = request.if_revision() {
        if file["version"].as_str() != Some(revision) {
            return Err(error("同步冲突：云端版本已改变"));
        }
    }
    if file["mimeType"].as_str() == Some(FOLDER) {
        let query = format!("'{id}' in parents and trashed = false");
        let page: Value = http_request(
            &ctx.http(),
            &token,
            "GET",
            &format!("{API}/files?q={}&fields=files(id)&pageSize=1&supportsAllDrives=true&includeItemsFromAllDrives=true", urlencoding::encode(&query)),
        )
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.json())
        .map_err(error)?;
        if page["files"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
        {
            return Err(error("文件夹不为空，无法删除"));
        }
    }
    http_request(
        &ctx.http(),
        &token,
        "DELETE",
        &format!("{API}/files/{id}?supportsAllDrives=true"),
    )
    .send()
    .and_then(|r| r.error_for_status())
    .map_err(error)?;
    Ok(())
}
