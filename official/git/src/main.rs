//! Fiex Git plugin runs as an independent process; never links to Fiex Core.
mod decorations;
mod files;
mod git_host;
mod history_host;
#[path = "../../../support/i18n.rs"]
mod i18n;
mod ignores;
mod remote_host;

use fiex_plugin_sdk::{
    self as sdk,
    filesystem::{FsEntry, FsListing, FsRead, FsRequest, VfsProvider},
    RpcError,
};
use serde_json::{json, Value};

fn param<'a>(p: &'a Value, name: &str) -> Result<&'a str, RpcError> {
    p.get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| RpcError::invalid_params(format!("missing {name}")))
}
fn string_list(p: &Value, name: &str) -> Result<Vec<String>, RpcError> {
    let values = p
        .get(name)
        .and_then(Value::as_array)
        .ok_or_else(|| RpcError::invalid_params(format!("missing {name}")))?;
    values
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_string)
                .ok_or_else(|| RpcError::invalid_params("invalid string list"))
        })
        .collect()
}
fn response<T: serde::Serialize>(result: Result<T, String>) -> Result<Value, RpcError> {
    serde_json::to_value(result.map_err(RpcError::internal)?)
        .map_err(|e| RpcError::internal(e.to_string()))
}

fn list(request: &FsRequest<'_>) -> Result<FsListing, RpcError> {
    let host = request.ctx();
    let path = request.node_id().unwrap_or("");
    let result = history_host::list(path, request.hidden(), &request.ignores(), host)
        .map_err(RpcError::internal)?;
    let start = request
        .params()
        .get("cursor")
        .and_then(Value::as_str)
        .unwrap_or("0")
        .parse::<usize>()
        .map_err(|_| RpcError::invalid_params("invalid cursor"))?;
    let limit = request
        .params()
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(200)
        .clamp(1, 500) as usize;
    let total = result.entries.len();
    if start > total {
        return Err(RpcError::invalid_params("cursor out of range"));
    }
    let end = (start + limit).min(total);
    Ok(FsListing {
        entries: result.entries[start..end]
            .iter()
            .cloned()
            .map(history_host::fs_entry)
            .collect(),
        next_cursor: (end < total).then(|| end.to_string()),
        warnings: result.warnings,
        truncated: false,
    })
}

fn stat(request: &FsRequest<'_>) -> Result<FsEntry, RpcError> {
    let path = request.node_id().unwrap_or("");
    history_host::info(path, request.ctx())
        .map(history_host::fs_entry)
        .map_err(RpcError::internal)
}

fn read(request: &FsRequest<'_>) -> Result<FsRead, RpcError> {
    let path = request
        .node_id()
        .ok_or_else(|| RpcError::invalid_params("missing nodeId"))?;
    let bytes = history_host::blob(path, request.ctx()).map_err(RpcError::internal)?;
    Ok(FsRead::from_bytes(bytes))
}

fn search(request: &FsRequest<'_>) -> Result<FsListing, RpcError> {
    let path = request
        .node_id()
        .ok_or_else(|| RpcError::invalid_params("missing nodeId"))?;
    let query = request
        .query()
        .ok_or_else(|| RpcError::invalid_params("missing query"))?;
    let limit = request
        .params()
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(500)
        .clamp(1, 2000) as usize;
    let mut entries = history_host::search(
        path,
        query,
        request.hidden(),
        &request.ignores(),
        request.ctx(),
    )
    .map_err(RpcError::internal)?;
    let truncated = entries.len() > limit;
    entries.truncate(limit);
    Ok(FsListing {
        entries: entries.into_iter().map(history_host::fs_entry).collect(),
        next_cursor: None,
        warnings: Vec::new(),
        truncated,
    })
}

/// Git history is a read-only standalone view: only list/stat/read/search are
/// declared, so Core never offers write, rename or remove.
struct GitView;

impl VfsProvider for GitView {
    fn operations(&self) -> &'static [sdk::filesystem::FsOperation] {
        &[
            sdk::filesystem::FsOperation::List,
            sdk::filesystem::FsOperation::Stat,
            sdk::filesystem::FsOperation::Read,
            sdk::filesystem::FsOperation::Search,
        ]
    }

    fn list(&self, request: &FsRequest<'_>) -> Result<FsListing, RpcError> {
        list(request)
    }

    fn stat(&self, request: &FsRequest<'_>) -> Result<FsEntry, RpcError> {
        stat(request)
    }

    fn read(&self, request: &FsRequest<'_>) -> Result<FsRead, RpcError> {
        read(request)
    }

    fn search(&self, request: &FsRequest<'_>) -> Result<FsListing, RpcError> {
        search(request)
    }
}

fn dispatch(ctx: sdk::RequestContext) -> Result<Value, RpcError> {
    let method = ctx.method();
    let p = ctx.params();
    let host = &ctx;
    let repository = serde_json::from_value::<sdk::ResourceRef>(p["root"].clone())
        .ok()
        .or_else(|| ctx.directory().map(|directory| directory.reference));
    let root = || {
        repository
            .as_ref()
            .ok_or_else(|| RpcError::invalid_params("missing repository ref"))
    };
    match method {
        "git.status" => Ok(json!(git_host::status(root()?, host))),
        "git.init" => response(git_host::init(root()?, host)),
        "git.refs" => response(git_host::refs(root()?, host)),
        "git.identity" => response(git_host::identity(root()?, host)),
        "git.stage" => response(git_host::stage(
            root()?,
            &string_list(p, "paths")?,
            p.get("all").and_then(Value::as_bool).unwrap_or(false),
            host,
        )),
        "git.unstage" => response(git_host::unstage(
            root()?,
            &string_list(p, "paths")?,
            p.get("all").and_then(Value::as_bool).unwrap_or(false),
            host,
        )),
        "git.commit" => response(git_host::commit(
            root()?,
            param(p, "message")?,
            param(p, "name")?,
            param(p, "email")?,
            host,
        )),
        "git.history" => response(history_host::history(
            root()?,
            p.get("skip").and_then(Value::as_u64).unwrap_or(0) as usize,
            host,
        )),
        "git.browse" => response(history_host::browse(root()?, param(p, "reference")?, host)),
        "git.remoteStatus" => response(remote_host::status(root()?, host)),
        "git.remoteSave" => response(remote_host::save(
            root()?,
            param(p, "name")?,
            param(p, "url")?,
            host,
        )),
        "git.remoteRemove" => response(remote_host::remove(root()?, param(p, "name")?, host)),
        "git.remoteSync" => response(remote_host::sync(
            root()?,
            param(p, "remote")?,
            p.get("push").and_then(Value::as_bool).unwrap_or(false),
            p.get("branch").and_then(Value::as_str),
            host,
        )),
        _ => Err(RpcError::method_not_found(method)),
    }
}

fn main() {
    let providers = sdk::Providers::new()
        .filesystem(GitView)
        .decorations(decorations::GitDecorations);
    if let Err(e) = providers.serve_with("org.fiex.git", dispatch) {
        eprintln!("Git plugin IPC error: {e}");
        std::process::exit(1);
    }
}
