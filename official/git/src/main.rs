//! Fiex Git plugin runs as an independent process; never links to Fiex Core.
mod decorations;
mod files;
mod git_host;
mod history_host;
mod ignores;
mod remote_host;

use fiex_plugin_sdk::{self as sdk, RpcError};
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
fn dispatch(ctx: sdk::RequestContext) -> Result<Value, RpcError> {
    let method = ctx.method();
    let p = ctx.params();
    let host = &ctx;
    let root = || param(p, "root");
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
        "git.historyAsset" => response(history_host::asset(param(p, "path")?, host)),
        "git.historyList" => response(history_host::list(
            param(p, "path")?,
            p.get("hidden").and_then(Value::as_bool).unwrap_or(false),
            &string_list(p, "ignores")?,
            host,
        )),
        "git.historyInfo" => response(history_host::info(param(p, "path")?, host)),
        "git.historyPreview" => response(history_host::preview(param(p, "path")?, host)),
        "git.historySearch" => response(history_host::search(
            param(p, "path")?,
            param(p, "query")?,
            p.get("hidden").and_then(Value::as_bool).unwrap_or(false),
            &string_list(p, "ignores")?,
            host,
        )),
        "git.remoteStatus" => response(remote_host::status(root()?, host)),
        "git.remoteSave" => response(remote_host::save(
            root()?,
            param(p, "name")?,
            param(p, "url")?,
            host,
        )),
        "git.remoteRemove" => response(remote_host::remove(root()?, param(p, "name")?, host)),
        "decorations.list" => decorations::list(p, host).map_err(RpcError::internal),
        "git.remoteSync" => response(remote_host::sync(
            root()?,
            param(p, "remote")?,
            p.get("push").and_then(Value::as_bool).unwrap_or(false),
            p.get("branch").and_then(Value::as_str),
            host,
        )),
        // Core routes plugin-owned schemes (declared in contributes.vfs)
        // through the same generic VFS methods for every plugin.
        "vfs.list" => {
            let path = param(p, "nodeId")?;
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
            let result =
                history_host::list(path, hidden, &ignores, host).map_err(RpcError::internal)?;
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
        "vfs.stat" => response(history_host::info(param(p, "nodeId")?, host)),
        "vfs.readSmall" => {
            use base64::Engine;
            let text =
                history_host::preview(param(p, "nodeId")?, host).map_err(RpcError::internal)?;
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
            let mut entries = history_host::search(
                param(p, "nodeId")?,
                param(p, "query")?,
                hidden,
                &ignores,
                host,
            )
            .map_err(RpcError::internal)?;
            let truncated = entries.len() > limit;
            entries.truncate(limit);
            Ok(json!({"entries": entries, "truncated": truncated}))
        }
        _ => Err(RpcError::method_not_found(method)),
    }
}
fn main() {
    if let Err(e) = sdk::serve("org.fiex.git", dispatch) {
        eprintln!("Git plugin IPC error: {e}");
        std::process::exit(1);
    }
}
