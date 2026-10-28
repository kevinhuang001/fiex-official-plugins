//! Google Drive v3 adapter. All HTTP goes through Core's authorized network
//! transport; the plugin has no filesystem, subprocess or socket permission.
mod auth;
mod mutate;
mod reads;
mod writes;
use auth::token;
use fiex_plugin_sdk::{
    self as sdk,
    filesystem::{FSProvider, FsEntry, FsListing, FsRead, FsRequest, FsWrite},
    RpcError,
};
use serde_json::{json, Value};

const API: &str = "https://www.googleapis.com/drive/v3";

fn error(e: impl std::fmt::Display) -> RpcError {
    RpcError::internal(e.to_string())
}

fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str, RpcError> {
    v[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| RpcError::invalid_params(format!("missing {key}")))
}

fn node(v: &str) -> Result<&str, RpcError> {
    if v.is_empty()
        || v.len() > 256
        || !v
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
    {
        return Err(RpcError::invalid_params("invalid Drive file ID"));
    }
    Ok(v)
}

fn http_request(
    http: &sdk::HttpClient,
    token: &str,
    method: &str,
    url: &str,
) -> sdk::http::Request {
    http.request(method, url).bearer_auth(token)
}

fn metadata(http: &sdk::HttpClient, token: &str, id: &str) -> Result<(Value, String), RpcError> {
    let response = http_request(http, token, "GET", &format!("{API}/files/{}?fields=id,name,mimeType,size,version,trashed,capabilities(canEdit)&supportsAllDrives=true", node(id)?))
        .send().and_then(|r| r.error_for_status()).map_err(error)?;
    let etag = response.headers.get("etag").cloned().unwrap_or_default();
    let value: Value = response.json().map_err(error)?;
    if value["trashed"].as_bool() == Some(true) {
        return Err(error("Drive file was trashed; local copy retained"));
    }
    Ok((value, etag))
}

/// One `FsEntry` from a Drive file resource. Drive IDs are stable per mount, so
/// they are the opaque `nodeId`; no host or Drive path is ever returned.
fn entry(file: &Value) -> FsEntry {
    let id = file["id"].as_str().unwrap_or("");
    let directory = file["mimeType"] == "application/vnd.google-apps.folder";
    let name = reads::safe_name(file["name"].as_str().unwrap_or("file"), id);
    let size = file["size"]
        .as_str()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let mut entry = if directory {
        FsEntry::directory(id, name.clone())
    } else {
        FsEntry::file(id, name.clone())
    };
    entry.size = Some(size);
    entry.modified = Some(0);
    entry.revision = file["version"].as_str().map(str::to_owned);
    entry.mime = file["mimeType"].as_str().map(str::to_owned);
    if !directory {
        entry.extension = name
            .rsplit_once('.')
            .map(|(_, extension)| extension.to_string());
    }
    entry
}

struct Drive;

impl FSProvider for Drive {
    fn operations(&self) -> &'static [sdk::filesystem::FsOp] {
        &[
            sdk::filesystem::FsOp::List,
            sdk::filesystem::FsOp::Stat,
            sdk::filesystem::FsOp::Read,
            sdk::filesystem::FsOp::Write,
            sdk::filesystem::FsOp::Mkdir,
            sdk::filesystem::FsOp::Rename,
            sdk::filesystem::FsOp::Remove,
        ]
    }

    fn list(&self, request: &FsRequest<'_>) -> Result<FsListing, RpcError> {
        reads::list(request)
    }

    fn stat(&self, request: &FsRequest<'_>) -> Result<FsEntry, RpcError> {
        reads::stat(request)
    }

    fn read(&self, request: &FsRequest<'_>) -> Result<FsRead, RpcError> {
        reads::read(request)
    }

    fn write(&self, request: &FsRequest<'_>) -> Result<FsWrite, RpcError> {
        writes::write(request)
    }

    fn mkdir(&self, request: &FsRequest<'_>) -> Result<FsEntry, RpcError> {
        mutate::mkdir(request)
    }

    fn rename(&self, request: &FsRequest<'_>) -> Result<FsEntry, RpcError> {
        mutate::rename(request)
    }

    fn remove(&self, request: &FsRequest<'_>) -> Result<(), RpcError> {
        mutate::remove(request)
    }
}

fn open(ctx: &sdk::RequestContext) -> Result<Value, RpcError> {
    let mount = ctx.params()["mountId"]
        .as_str()
        .ok_or_else(|| RpcError::invalid_params("missing mountId"))?;
    Ok(json!({"kind":"vfs","mountId":mount,"nodeRef":{"mountId":mount,"nodeId":""}}))
}

impl sdk::ConnectionProvider for Drive {
    fn open(&self, ctx: &sdk::RequestContext) -> Result<serde_json::Value, sdk::RpcError> {
        open(ctx)
    }
    fn validate(&self, ctx: &sdk::RequestContext) -> Result<serde_json::Value, sdk::RpcError> {
        auth::validate(ctx)
    }
}

fn main() {
    let providers = sdk::Providers::new().filesystem(Drive).connection(Drive);
    if let Err(e) = providers.serve("org.fiex.gdrive") {
        eprintln!("Drive IPC failed: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ids_are_not_paths_or_urls() {
        assert!(node("abc_123-Z").is_ok());
        for value in ["", "../secret", "https://evil", "a/b", "a' or true"] {
            assert!(node(value).is_err());
        }
    }
}
