//! Windows filename search backed by the Everything index.
//!
//! Everything owns the index; this plugin only queries its IPC endpoint and
//! maps results into Fiex's search entry shape. No file content is read here.
use fiex_plugin_sdk::{self as sdk, RpcError};
#[cfg(windows)]
use serde::Serialize;
use serde_json::{json, Value};

#[cfg(windows)]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    path: String,
    name: String,
    is_dir: bool,
    is_symlink: bool,
    size: u64,
    modified: u64,
    created: u64,
    extension: String,
    mime: String,
    readonly: bool,
    link_target: Option<String>,
    tags: Vec<Value>,
    sync: Option<String>,
}

#[cfg(windows)]
fn params(p: &Value) -> (String, String, bool, usize) {
    let query = p
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let root = p
        .get("root")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let hidden = p.get("hidden").and_then(Value::as_bool).unwrap_or(false);
    let limit = p
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(200)
        .clamp(1, 500) as usize;
    (query, root, hidden, limit)
}

#[cfg(windows)]
fn entry(path: &std::path::Path, name: &str, is_dir: bool, size: u64) -> Entry {
    Entry {
        path: path.to_string_lossy().into_owned(),
        name: name.to_string(),
        is_dir,
        is_symlink: false,
        size,
        modified: 0,
        created: 0,
        extension: path
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase(),
        mime: mime_guess::from_path(path)
            .first_or_octet_stream()
            .to_string(),
        readonly: false,
        link_target: None,
        tags: Vec::new(),
        sync: None,
    }
}

#[cfg(windows)]
fn query(p: &Value) -> Result<Value, RpcError> {
    use everything_ipc::wm::{EverythingClient, RequestFlags};

    let (query, root, hidden, limit) = params(p);
    if query.is_empty() {
        return Ok(json!({"entries": [], "truncated": false}));
    }
    let client = EverythingClient::new()
        .map_err(|error| RpcError::internal(format!("Everything 未运行或不可用：{error}")))?;
    let result = client
        .query_wait(&query)
        .request_flags(
            RequestFlags::FileName
                | RequestFlags::Path
                | RequestFlags::Size
                | RequestFlags::Attributes,
        )
        .max_results(limit as u32)
        .call()
        .map_err(|error| RpcError::internal(format!("Everything 查询失败：{error}")))?;
    let mut entries = Vec::new();
    for item in result.iter() {
        let Some(name) = item.get_string(RequestFlags::FileName) else {
            continue;
        };
        if !hidden && name.starts_with('.') {
            continue;
        }
        let Some(parent) = item.get_str(RequestFlags::Path) else {
            continue;
        };
        let path = std::path::PathBuf::from(parent.to_string_lossy()).join(&name);
        if !root.is_empty() && root != "everywhere://" && !path.starts_with(&root) {
            continue;
        }
        let attributes = item.get_u32(RequestFlags::Attributes).unwrap_or_default();
        let is_dir = attributes & 0x10 != 0;
        let size = item.get_size(RequestFlags::Size).unwrap_or_default();
        entries.push(entry(&path, &name, is_dir, size));
    }
    let truncated = entries.len() >= limit;
    entries.truncate(limit);
    Ok(json!({"entries": entries, "truncated": truncated}))
}

#[cfg(not(windows))]
fn query(_p: &Value) -> Result<Value, RpcError> {
    Ok(json!({
        "entries": [],
        "truncated": false,
        "warning": "Everything 搜索只在 Windows 上可用"
    }))
}

fn dispatch(method: &str, p: Value) -> Result<Value, RpcError> {
    match method {
        "search.query" => query(&p),
        "search.status" => Ok(json!({
            "available": cfg!(windows),
            "provider": "everything",
            "indexOwner": "Everything"
        })),
        "search.rebuild" => Err(RpcError::invalid_params(
            "Everything 的索引由 Everything 自身维护，Fiex 不重建",
        )),
        _ => Err(RpcError::method_not_found(method)),
    }
}

fn main() {
    if let Err(error) = sdk::serve("org.fiex.everything", dispatch) {
        eprintln!("Everything search plugin failed: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(windows))]
    #[test]
    fn non_windows_query_is_an_empty_result() {
        let result = dispatch("search.query", json!({"query": "fiex"})).unwrap();
        assert_eq!(result["entries"], json!([]));
        assert_eq!(result["truncated"], false);
    }

    #[test]
    fn status_reports_the_platform() {
        let result = dispatch("search.status", json!({})).unwrap();
        assert_eq!(result["provider"], "everything");
        assert_eq!(result["available"], cfg!(windows));
    }

    #[test]
    fn rebuild_is_owned_by_everything() {
        assert!(dispatch("search.rebuild", json!({})).is_err());
    }
}
