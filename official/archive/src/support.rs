use super::{archives, listing};
use fiex_plugin_sdk::{self as sdk, ProviderInvocationExt, RpcError};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

/// How long a `beforeOpen` hook waits for an extraction that is likely quick.
/// If the archive is still being expanded after this, the hook returns so Core
/// can finish the open immediately; the task keeps running in the background.
const HOOK_SYNC_GRACE: Duration = Duration::from_millis(750);
pub(super) fn param<'a>(p: &'a Value, k: &str) -> Result<&'a str, RpcError> {
    p.get(k)
        .and_then(Value::as_str)
        .ok_or_else(|| RpcError::invalid_params(format!("missing {k}")))
}
/// Stored as a JSON array by the list setting; older installs may still hold a
/// comma/newline separated string, which is accepted for compatibility.
pub(super) fn stored_ignores(host: &dyn sdk::ProviderInvocation, key: &str) -> Vec<String> {
    match host.storage().get::<Value>(key) {
        Ok(Some(Value::Array(items))) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(str::to_string)
            .collect(),
        Ok(Some(Value::String(value))) => value
            .split(['\n', ',', '，'])
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}
pub(super) fn merge_ignores(params: &Value, mut stored: Vec<String>) -> Vec<String> {
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
pub(super) fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

pub(super) fn max_total_bytes(ctx: &dyn sdk::ProviderInvocation, p: &Value) -> u64 {
    const GIB: u64 = 1024 * 1024 * 1024;
    let configured = ctx
        .storage()
        .get::<Value>("extractMaxTotalGiB")
        .ok()
        .flatten()
        .and_then(|value| value.as_u64())
        .unwrap_or(archives::DEFAULT_MAX_TOTAL_GIB);
    let bytes = p
        .get("maxTotalBytes")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| configured.saturating_mul(GIB));
    bytes.clamp(GIB, archives::MAX_TOTAL_CAP_GIB.saturating_mul(GIB))
}

pub(super) fn archive_error(error: String) -> RpcError {
    if error == archives::CANCELLED {
        RpcError::cancelled()
    } else {
        RpcError::internal(error)
    }
}

/// Extraction errors are surfaced to the status/error channel. An encrypted
/// archive is not a crash: point the user at the explicit extract/preview
/// actions that can carry a password.
pub(super) fn hook_error_message(error: &str) -> String {
    if listing::looks_encrypted(error) {
        crate::i18n::t("压缩包已加密，请使用右键“解压缩”或预览面板输入密码").into()
    } else {
        error.to_string()
    }
}

pub(super) fn hook_extract(
    path: &sdk::ResourceRef,
    destination: &sdk::ResourceRef,
    password: Option<&str>,
    ignores: &[String],
    max_total: u64,
    context: &dyn archives::ArchiveContext,
) -> Result<sdk::ResourceRef, String> {
    archives::extract(path, destination, password, ignores, max_total, context)
}

/// `beforeOpen` consumes the OS open. Extraction runs as a background task so
/// a large archive cannot stall the open, and the response intentionally never
/// contains the task id or a raw task payload.
pub(super) fn before_open(
    path: &sdk::ResourceRef,
    params: &Value,
    host: &dyn sdk::ProviderInvocation,
) -> Result<sdk::contracts::HookResult, RpcError> {
    let stored = stored_ignores(host, "extractIgnores");
    let ignores = merge_ignores(params, stored);
    let password = params.get("password").and_then(Value::as_str);
    let max_total = max_total_bytes(host, params);
    let task_path = path.clone();
    let destination = host
        .directory()
        .ok_or_else(|| RpcError::invalid_params("missing output directory ref"))?
        .reference
        .clone();
    let task_destination = destination.clone();
    let task_ignores = ignores.clone();
    let task_password = password.map(str::to_owned);
    match host.spawn_task_with_label(crate::i18n::t("正在解压"), move |task| {
        hook_extract(
            &task_path,
            &task_destination,
            task_password.as_deref(),
            &task_ignores,
            max_total,
            &task,
        )
        .map(|destination| json!({ "destination": destination }))
        .map_err(|error| {
            if error != archives::CANCELLED {
                let _ =
                    task.report_error("archive.hook.extract", &hook_error_message(&error), None);
            }
            archive_error(error)
        })
    }) {
        Ok(handle) => {
            // Small archives finish inside the grace window, so the caller's
            // immediate reload already shows the extracted folder. Large ones
            // keep running in the status centre. Both consume the open and
            // return only the handled flag, never task-shaped JSON.
            let deadline = Instant::now() + HOOK_SYNC_GRACE;
            while !handle.is_finished() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(sdk::contracts::HookResult { handled: true })
        }
        Err(error) => {
            let _ = host.log(
                "warn",
                "Background extraction unavailable; trying inline extraction",
            );
            let _ = error;
            // The task registry is exhausted or unavailable. Extract inline so
            // the user's open is still consumed, then report any real failure.
            match hook_extract(
                path,
                &destination,
                password,
                &ignores,
                max_total,
                host.sdk_context(),
            ) {
                Ok(_) => Ok(sdk::contracts::HookResult { handled: true }),
                Err(error) => {
                    if error != archives::CANCELLED {
                        let _ = host.report_error(
                            "archive.hook.extract",
                            &hook_error_message(&error),
                            None,
                        );
                    }
                    Ok(sdk::contracts::HookResult { handled: true })
                }
            }
        }
    }
}
