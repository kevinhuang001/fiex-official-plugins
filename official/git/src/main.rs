//! Fiex Git plugin runs as an independent process; never links to Fiex Core.
mod files;
mod git;
mod git_history;
mod git_remote;
mod ignores;

use fiex_plugin_sdk::{self as sdk, RpcError};
use serde_json::{json, Value};

fn param<'a>(p: &'a Value, name: &str) -> Result<&'a str, RpcError> {
    p.get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| RpcError::invalid_params(format!("missing {name}")))
}
fn domain_list(p: &Value) -> Vec<String> {
    p.get("networkDomains")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
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
fn dispatch(method: &str, p: Value) -> Result<Value, RpcError> {
    let root = || param(&p, "root");
    match method {
        "git.status" => Ok(json!(git::status(root()?))),
        "git.init" => response(git::init(root()?)),
        "git.refs" => response(git::refs(root()?)),
        "git.identity" => response(Ok(git::identity(root()?))),
        "git.stage" => response(git::stage(
            root()?,
            &string_list(&p, "paths")?,
            p.get("all").and_then(Value::as_bool).unwrap_or(false),
        )),
        "git.unstage" => response(git::unstage(
            root()?,
            &string_list(&p, "paths")?,
            p.get("all").and_then(Value::as_bool).unwrap_or(false),
        )),
        "git.commit" => response(git::commit(
            root()?,
            param(&p, "message")?,
            param(&p, "name")?,
            param(&p, "email")?,
        )),
        "git.history" => response(git_history::history(
            root()?,
            p.get("skip").and_then(Value::as_u64).unwrap_or(0) as usize,
        )),
        "git.browse" => response(git_history::browse(root()?, param(&p, "reference")?)),
        "git.historyAsset" => response(git_history::asset(param(&p, "path")?)),
        "git.historyList" => response(git_history::list(
            param(&p, "path")?,
            p.get("hidden").and_then(Value::as_bool).unwrap_or(false),
            &string_list(&p, "ignores")?,
        )),
        "git.historyInfo" => response(git_history::info(param(&p, "path")?)),
        "git.historyPreview" => response(git_history::preview(param(&p, "path")?)),
        "git.historySearch" => response(git_history::search(
            param(&p, "path")?,
            param(&p, "query")?,
            p.get("hidden").and_then(Value::as_bool).unwrap_or(false),
            &string_list(&p, "ignores")?,
        )),
        "git.remoteStatus" => response(git_remote::status(root()?)),
        "git.remoteSave" => response(git_remote::save(
            root()?,
            param(&p, "name")?,
            param(&p, "url")?,
            &domain_list(&p),
        )),
        "git.remoteRemove" => response(git_remote::remove(root()?, param(&p, "name")?)),
        "git.remoteSync" => response(git_remote::sync(
            root()?,
            param(&p, "remote")?,
            p.get("push").and_then(Value::as_bool).unwrap_or(false),
            p.get("branch").and_then(Value::as_str),
            &domain_list(&p),
        )),
        // Core routes plugin-owned schemes (declared in contributes.vfs)
        // through the same generic VFS methods for every plugin.
        "vfs.list" => {
            let path = param(&p, "nodeId")?;
            let hidden = p.get("hidden").and_then(Value::as_bool).unwrap_or(false);
            let ignores = p
                .get("ignores")
                .and_then(Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let result = git_history::list(path, hidden, &ignores).map_err(RpcError::internal)?;
            let start = p
                .get("cursor")
                .and_then(Value::as_str)
                .unwrap_or("0")
                .parse::<usize>()
                .map_err(|_| RpcError::invalid_params("invalid cursor"))?;
            let limit = p
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(200)
                .clamp(1, 500) as usize;
            if start > result.entries.len() {
                return Err(RpcError::invalid_params("cursor out of range"));
            }
            let end = (start + limit).min(result.entries.len());
            let entries = result.entries[start..end].to_vec();
            Ok(json!({
                "entries": entries,
                "nextCursor": if end < result.entries.len() { Some(end.to_string()) } else { None },
                "snapshotToken": serde_json::Value::Null,
                "warnings": result.warnings
            }))
        }
        "vfs.stat" => response(git_history::info(param(&p, "nodeId")?)),
        "vfs.readSmall" => {
            use base64::Engine;
            let text = git_history::preview(param(&p, "nodeId")?).map_err(RpcError::internal)?;
            let bytes = text.into_bytes();
            let max = p
                .get("maxBytes")
                .and_then(Value::as_u64)
                .unwrap_or(262144)
                .clamp(1, 1024 * 1024) as usize;
            let bytes = if bytes.len() > max {
                return Err(RpcError::invalid_params("preview exceeds requested size"));
            } else {
                bytes
            };
            Ok(json!({
                "base64": base64::engine::general_purpose::STANDARD.encode(&bytes),
                "length": bytes.len()
            }))
        }
        "vfs.search" => {
            let hidden = p.get("hidden").and_then(Value::as_bool).unwrap_or(false);
            let ignores = p
                .get("ignores")
                .and_then(Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let limit = p
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(500)
                .clamp(1, 2000) as usize;
            let mut entries =
                git_history::search(param(&p, "nodeId")?, param(&p, "query")?, hidden, &ignores)
                    .map_err(RpcError::internal)?;
            let truncated = entries.len() > limit;
            entries.truncate(limit);
            Ok(json!({"entries": entries, "truncated": truncated}))
        }
        _ => Err(RpcError::method_not_found(method)),
    }
}
fn main() {
    if let Err(e) = sdk::serve_with_manifest(
        "org.fiex.git",
        include_str!("../manifest.json"), dispatch) {
        eprintln!("Git plugin IPC error: {e}");
        std::process::exit(1);
    }
}
