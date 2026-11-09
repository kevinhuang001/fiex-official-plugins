use super::*;
use fiex_plugin_sdk::{self as sdk, ProviderInvocationExt, RpcError};
use serde_json::{Value, json};

pub(super) fn run_operation(
    ctx: &dyn sdk::ProviderInvocation,
    params: Value,
    operation: &str,
) -> Result<Value, RpcError> {
    let method = operation;
    let p = &params;
    let host = ctx;
    match operation {
        "archive.compress" => {
            let paths = if let Some(paths) = p.get("paths").filter(|value| value.is_array()) {
                serde_json::from_value::<Vec<sdk::ResourceRef>>(paths.clone())
                    .map_err(|error| RpcError::invalid_params(error.to_string()))?
            } else {
                ctx.targets()
                    .iter()
                    .map(|target| target.reference.clone())
                    .collect()
            };
            let stored = stored_ignores(host, "compressIgnores");
            let ignores = merge_ignores(p, stored);
            let format = param(p, "format")?.to_string();
            let destination = match p.get("destination").filter(|value| value.is_object()) {
                Some(value) => serde_json::from_value::<sdk::ResourceRef>(value.clone())
                    .map_err(|error| RpcError::invalid_params(error.to_string()))?,
                None => {
                    if let Some(name) = p.get("destinationName").and_then(Value::as_str) {
                        host.files()
                            .child(
                                &host
                                    .directory()
                                    .ok_or_else(|| {
                                        RpcError::invalid_params("missing output directory ref")
                                    })?
                                    .reference,
                                name,
                            )
                            .map_err(|error| RpcError::internal(error.to_string()))?
                    } else {
                        archives::default_destination(&paths, &format, host.sdk_context())
                            .map_err(RpcError::internal)?
                    }
                }
            };
            let level = p.get("level").and_then(Value::as_u64).map(|n| n as u32);
            let password = p.get("password").and_then(Value::as_str).map(str::to_owned);
            let task = ctx.spawn_task_with_label(crate::i18n::t("正在压缩"), move |task| {
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
        "archive.list" => match listing::list(
            &resources::from_params(p, "path", host)?.reference,
            p.get("password").and_then(Value::as_str),
            host.sdk_context(),
        ) {
            Ok(value) => Ok(value),
            Err(message) if listing::looks_encrypted(&message) => Ok(json!({
                "supported": true, "encrypted": true, "passwordRequired": true, "entries": []
            })),
            Err(message) => Err(RpcError::internal(message)),
        },
        // Runs before Core opens the file: consume the open and extract into
        // the archive's folder. Never fall through for a matching archive, and
        // never mistake this event's response for a menu/task result.
        "archive.extract" => {
            let stored = stored_ignores(host, "extractIgnores");
            let ignores = merge_ignores(p, stored);
            let path = resources::from_params(p, "path", host)?.reference;
            let destination =
                if let Some(value) = p.get("destination").filter(|value| value.is_object()) {
                    serde_json::from_value::<sdk::ResourceRef>(value.clone())
                        .map_err(|error| RpcError::invalid_params(error.to_string()))?
                } else {
                    host.directory()
                        .ok_or_else(|| RpcError::invalid_params("missing output directory ref"))?
                        .reference
                        .clone()
                };
            let password = p.get("password").and_then(Value::as_str).map(str::to_owned);
            let max_total = max_total_bytes(host, p);
            let task = ctx.spawn_task_with_label(crate::i18n::t("正在解压"), move |task| {
                let result = archives::extract(
                    &path,
                    &destination,
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
