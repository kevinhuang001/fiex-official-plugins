mod archives;
mod ignores;
mod listing;
use fiex_plugin_sdk::{self as sdk, RpcError};
use serde_json::{json, Value};
fn param<'a>(p: &'a Value, k: &str) -> Result<&'a str, RpcError> {
    p.get(k)
        .and_then(Value::as_str)
        .ok_or_else(|| RpcError::invalid_params(format!("missing {k}")))
}
/// Stored as a JSON array by the list setting; older installs may still hold a
/// comma/newline separated string, which is accepted for compatibility.
fn stored_ignores(host: &sdk::RequestContext, key: &str) -> Vec<String> {
    match host.storage_get("global", key) {
        Ok(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(str::to_string)
            .collect(),
        Ok(Value::String(value)) => value
            .split(['\n', ',', '，'])
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}
fn merge_ignores(params: &Value, mut stored: Vec<String>) -> Vec<String> {
    let mut values = params
        .get("ignores")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    values.append(&mut stored);
    values
}
fn max_total_bytes(ctx: &sdk::RequestContext, p: &Value) -> u64 {
    const GIB: u64 = 1024 * 1024 * 1024;
    let configured = ctx
        .storage_get("global", "extractMaxTotalGiB")
        .ok()
        .and_then(|value| value.as_u64())
        .unwrap_or(archives::DEFAULT_MAX_TOTAL_GIB);
    let bytes = p
        .get("maxTotalBytes")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| configured.saturating_mul(GIB));
    bytes.clamp(GIB, archives::MAX_TOTAL_CAP_GIB.saturating_mul(GIB))
}

fn archive_error(error: String) -> RpcError {
    if error == archives::CANCELLED {
        RpcError::cancelled()
    } else {
        RpcError::internal(error)
    }
}
fn run(ctx: sdk::RequestContext) -> Result<Value, RpcError> {
    let method = ctx.method();
    let p = ctx.params();
    let host = &ctx;
    match method {
        "archive.compress" => {
            let paths = p
                .get("paths")
                .and_then(Value::as_array)
                .ok_or_else(|| RpcError::invalid_params("missing paths"))?
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(str::to_string)
                        .ok_or_else(|| RpcError::invalid_params("invalid path"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let use_settings = p
                .get("ignoreSettings")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let stored = if use_settings {
                stored_ignores(host, "compressIgnores")
            } else {
                Vec::new()
            };
            let ignores = merge_ignores(p, stored);
            let format = param(p, "format")?.to_string();
            let destination = match p.get("destination").and_then(Value::as_str) {
                Some(value) => value.to_owned(),
                None => archives::default_destination(&paths, &format, host)
                    .map_err(RpcError::internal)?,
            };
            let level = p.get("level").and_then(Value::as_u64).map(|n| n as u32);
            let password = p.get("password").and_then(Value::as_str).map(str::to_owned);
            let task = ctx.spawn_task_with_label("正在压缩", move |task| {
                let result = archives::compress(
                    &paths,
                    &destination,
                    &format,
                    &ignores,
                    level,
                    password.as_deref(),
                    &task,
                )
                .map_err(archive_error)?;
                Ok(json!(result))
            })?;
            Ok(json!({"taskId": task.id()}))
        }
        "archive.list" => Ok(listing::list(
            param(p, "path")?,
            p.get("password").and_then(Value::as_str),
            host,
        )
        .map_err(RpcError::internal)?),
        // Runs before Core opens the file: extract in place, never fall through
        // for a valid archive. A password-protected archive hands control to
        // the preview panel so the user can type the password.
        "archive.beforeOpen" => {
            let path = param(p, "path")?;
            let stored = stored_ignores(host, "extractIgnores");
            let ignores = merge_ignores(p, stored);
            let password = p.get("password").and_then(Value::as_str);
            let max_total = max_total_bytes(host, p);
            match archives::extract(path, None, password, &ignores, max_total, host) {
                Ok(destination) => Ok(json!({ "handled": true, "destination": destination })),
                Err(error) if listing::looks_encrypted(&error) => {
                    Ok(json!({ "handled": true, "preview": true }))
                }
                Err(error) => Err(RpcError::internal(error)),
            }
        }
        "archive.extract" => {
            let stored = stored_ignores(host, "extractIgnores");
            let ignores = merge_ignores(p, stored);
            let path = param(p, "path")?.to_string();
            let destination = p
                .get("destination")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let password = p.get("password").and_then(Value::as_str).map(str::to_owned);
            let max_total = max_total_bytes(host, p);
            let task = ctx.spawn_task_with_label("正在解压", move |task| {
                let result = archives::extract(
                    &path,
                    destination.as_deref(),
                    password.as_deref(),
                    &ignores,
                    max_total,
                    &task,
                )
                .map_err(archive_error)?;
                Ok(json!(result))
            })?;
            Ok(json!({"taskId": task.id()}))
        }
        _ => Err(RpcError::method_not_found(method)),
    }
}
fn main() {
    if let Err(e) = sdk::serve("org.fiex.archive", run) {
        eprintln!("Archive plugin failed: {e}");
        std::process::exit(1);
    }
}
