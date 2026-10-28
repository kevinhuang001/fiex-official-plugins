mod archives;
#[path = "../../../support/i18n.rs"]
mod i18n;
mod ignores;
mod listing;
#[path = "../../../support/resources.rs"]
mod resources;
use fiex_plugin_sdk::{self as sdk, RpcError};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

/// How long a `beforeOpen` hook waits for an extraction that is likely quick.
/// If the archive is still being expanded after this, the hook returns so Core
/// can finish the open immediately; the task keeps running in the background.
const HOOK_SYNC_GRACE: Duration = Duration::from_millis(750);
fn param<'a>(p: &'a Value, k: &str) -> Result<&'a str, RpcError> {
    p.get(k)
        .and_then(Value::as_str)
        .ok_or_else(|| RpcError::invalid_params(format!("missing {k}")))
}
/// Stored as a JSON array by the list setting; older installs may still hold a
/// comma/newline separated string, which is accepted for compatibility.
fn stored_ignores(host: &sdk::RequestContext, key: &str) -> Vec<String> {
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
fn format_size(bytes: u64) -> String {
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

fn max_total_bytes(ctx: &sdk::RequestContext, p: &Value) -> u64 {
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

fn archive_error(error: String) -> RpcError {
    if error == archives::CANCELLED {
        RpcError::cancelled()
    } else {
        RpcError::internal(error)
    }
}

/// Extraction errors are surfaced to the status/error channel. An encrypted
/// archive is not a crash: point the user at the explicit extract/preview
/// actions that can carry a password.
fn hook_error_message(error: &str) -> String {
    if listing::looks_encrypted(error) {
        crate::i18n::t("压缩包已加密，请使用右键“解压缩”或预览面板输入密码").into()
    } else {
        error.to_string()
    }
}

fn hook_extract(
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
fn before_open(
    path: &sdk::ResourceRef,
    params: &Value,
    host: &sdk::RequestContext,
) -> Result<Value, RpcError> {
    let stored = stored_ignores(host, "extractIgnores");
    let ignores = merge_ignores(params, stored);
    let password = params.get("password").and_then(Value::as_str);
    let max_total = max_total_bytes(host, params);
    let task_path = path.clone();
    let destination = host
        .directory()
        .ok_or_else(|| RpcError::invalid_params("missing output directory ref"))?
        .reference;
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
            let _ = task.report_error("archive.hook.extract", &hook_error_message(&error), None);
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
            Ok(json!({ "handled": true }))
        }
        Err(error) => {
            let _ = host.report_error("archive.hook.start", &error.message, None);
            // The task registry is exhausted or unavailable. Extract inline so
            // the user's open is still consumed, then report any real failure.
            match hook_extract(path, &destination, password, &ignores, max_total, host) {
                Ok(_) => Ok(json!({ "handled": true })),
                Err(error) => {
                    let _ = host.report_error(
                        "archive.hook.extract",
                        &hook_error_message(&error),
                        None,
                    );
                    Ok(json!({ "handled": true }))
                }
            }
        }
    }
}
fn archive_menu(params: &Value) -> Value {
    let parent = params.get("parentId").and_then(Value::as_str);
    if let Some(parent) = parent {
        if parent != "archive.compress" {
            return json!({"items":[]});
        }
        let formats = [
            ("archive.zip", "ZIP"),
            ("archive.rar", "RAR"),
            ("archive.7z", "7z"),
            ("archive.tar", "TAR"),
            ("archive.tar.gz", "TAR.GZ"),
            ("archive.tar.bz2", "TAR.BZ2"),
            ("archive.tar.xz", "TAR.XZ"),
            ("archive.advanced", crate::i18n::t("高级压缩…")),
        ];
        return json!({"items":formats.into_iter().map(|(id, title)| {
            json!({"id":id,"title":title,"kind":"action"})
        }).collect::<Vec<_>>()});
    }

    let targets = params.get("targets").and_then(Value::as_array);
    let selection = targets.map_or(0, Vec::len);
    if selection == 0 {
        return json!({"items":[]});
    }
    let archive = |name: &str| {
        [
            "zip", "rar", "7z", "tar", "gz", "tgz", "bz2", "tbz2", "xz", "txz",
        ]
        .iter()
        .any(|ext| name.to_ascii_lowercase().ends_with(&format!(".{ext}")))
    };
    let only_archive = selection == 1
        && targets
            .and_then(|targets| targets.first())
            .and_then(|target| target.pointer("/metadata/name"))
            .and_then(Value::as_str)
            .is_some_and(archive);
    if only_archive {
        return json!({"items":[
            {"id":"archive.extract","title":crate::i18n::t("解压缩"),"kind":"action"}
        ]});
    }
    json!({"items":[{"id":"archive.compress","title":crate::i18n::t("压缩"),"kind":"submenu"}]})
}

fn archive_info(p: &Value, host: &sdk::RequestContext) -> Result<Value, RpcError> {
    let target = resources::input(host, "path")?;
    let path = &target.reference;
    let password = p.get("password").and_then(Value::as_str);
    let listing = listing::list(path, password, host).map_err(RpcError::internal)?;
    let compressed = host
        .files()
        .metadata(path)
        .map(|entry| entry.size.unwrap_or(0))
        .unwrap_or(0);
    let entries = listing
        .get("entries")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let count = entries.len();
    let uncompressed = entries
        .iter()
        .filter_map(|entry| entry.get("size").and_then(Value::as_u64))
        .sum::<u64>();
    let format = resources::name(&target)?
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_uppercase())
        .unwrap_or_else(|| "UNKNOWN".into());
    let mut rows = vec![
        json!({"label":crate::i18n::t("格式"),"value":format}),
        json!({"label":crate::i18n::t("压缩后大小"),"value":format_size(compressed)}),
        json!({"label":crate::i18n::t("解压后大小"),"value":format_size(uncompressed)}),
        json!({"label":crate::i18n::t("文件数"),"value":count.to_string()}),
    ];
    if uncompressed > 0 {
        let saved = 1.0 - compressed as f64 / uncompressed as f64;
        rows.push(
            json!({"label":crate::i18n::t("压缩率"),"value":format!("{:.1}%", saved * 100.0)}),
        );
    }
    if listing
        .get("encrypted")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        rows.push(json!({"label":crate::i18n::t("加密"),"value":crate::i18n::t("是")}));
    }
    if listing.get("supported").and_then(Value::as_bool) == Some(false) {
        if let Some(message) = listing.get("message").and_then(Value::as_str) {
            rows.push(json!({"label":crate::i18n::t("说明"),"value":message}));
        }
    }
    Ok(json!({"rows": rows}))
}

fn run_operation(
    ctx: &sdk::RequestContext,
    params: Value,
    operation: &str,
) -> Result<Value, RpcError> {
    let method = operation;
    let p = &params;
    let host = &ctx;
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
                        archives::default_destination(&paths, &format, host)
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
        "archive.info" => archive_info(p, host),
        "archive.list" => match listing::list(
            &resources::input(host, "path")?.reference,
            p.get("password").and_then(Value::as_str),
            host,
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
            let path = resources::input(host, "path")?.reference;
            let destination =
                if let Some(value) = p.get("destination").filter(|value| value.is_object()) {
                    serde_json::from_value::<sdk::ResourceRef>(value.clone())
                        .map_err(|error| RpcError::invalid_params(error.to_string()))?
                } else {
                    host.directory()
                        .ok_or_else(|| RpcError::invalid_params("missing output directory ref"))?
                        .reference
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
fn main() {
    if let Err(e) = sdk::Providers::new()
        .info(Archive)
        .preview(Archive)
        .decorations(Archive)
        .context_menu(Archive)
        .hooks(Archive)
        .serve_with("org.fiex.archive", |ctx| {
            run_operation(&ctx, ctx.params().clone(), ctx.method())
        })
    {
        eprintln!("Archive plugin failed: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod context_menu_tests {
    use super::*;

    #[test]
    fn compression_options_are_resolved_only_after_expanding() {
        let targets = json!({"targets":[{"metadata":{"name":"notes.txt"}}]});
        let root = archive_menu(&targets);
        let items = root["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["kind"], "submenu");
        assert_eq!(items[0]["id"], "archive.compress");
        let children = archive_menu(&json!({
            "targets":targets["targets"],
            "parentId":"archive.compress"
        }));
        let items = children["items"].as_array().unwrap();
        assert!(items.len() >= 2);
        assert!(items.iter().all(|item| item.get("parentId").is_none()));
        assert!(items.iter().any(|item| item["id"] == "archive.zip"));
    }

    #[test]
    fn encrypted_hook_failures_point_at_the_password_actions() {
        assert!(hook_error_message("Password required").contains("密码"));
        assert!(hook_error_message("archive is encrypted").contains("预览"));
        assert_eq!(hook_error_message("磁盘已满"), "磁盘已满");
    }

    #[test]
    fn manifest_negotiates_a_longer_ui_budget_for_archive_listing() {
        let manifest: Value = serde_json::from_str(include_str!("../manifest.json")).unwrap();
        let timeout = manifest["extensions"]["ui"]["timeoutMs"]
            .as_u64()
            .expect("manifest must declare extensions.ui.timeoutMs");
        assert!(
            timeout >= 120_000,
            "archive listing needs a longer budget than Core's old default"
        );
    }

    #[test]
    fn extracting_archive_stays_a_single_action() {
        let root = archive_menu(&json!({"targets":[{"metadata":{"name":"sample.zip"}}]}));
        assert_eq!(root["items"][0]["id"], "archive.extract");
        assert_eq!(root["items"].as_array().unwrap().len(), 1);
        assert!(archive_menu(&json!({"targets":[]}))["items"]
            .as_array()
            .unwrap()
            .is_empty());
        assert!(archive_menu(&json!({"parentId":"foreign"}))["items"]
            .as_array()
            .unwrap()
            .is_empty());
    }
}

#[path = "../../../support/provider_results.rs"]
mod provider_results;
struct Archive;
impl sdk::InfoProvider for Archive {
    fn resolve(&self, ctx: &sdk::RequestContext) -> Result<Vec<sdk::InfoSection>, RpcError> {
        let target = ctx
            .target()
            .ok_or_else(|| RpcError::invalid_params("missing target"))?;
        let rows = archive_info(&json!({"path":target.reference}), ctx)?;
        provider_results::section("archive", crate::i18n::t("压缩包"), rows["rows"].clone())
    }
}
impl sdk::PreviewProvider for Archive {
    fn open(&self, _ctx: &sdk::RequestContext) -> Result<sdk::PreviewView, RpcError> {
        Ok(sdk::PreviewView::html("ui/dist/preview.html"))
    }
}
impl sdk::DecorationProvider for Archive {
    fn resolve(&self, ctx: &sdk::RequestContext) -> Result<Vec<Value>, RpcError> {
        let mut decorations = Vec::new();
        {
            for target in ctx.targets() {
                let Some(metadata) = target.metadata.as_ref() else {
                    continue;
                };
                let entry = serde_json::to_value(metadata)
                    .map_err(|e| RpcError::internal(e.to_string()))?;
                let Some(name) = entry.get("name").and_then(Value::as_str) else {
                    continue;
                };
                if entry["kind"] == "directory" {
                    continue;
                }
                let extension = entry
                    .get("extension")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_ascii_lowercase();
                let label = match extension.as_str() {
                    "zip" => "ZIP",
                    "rar" => "RAR",
                    "7z" => "7Z",
                    "tar" => "TAR",
                    "gz" | "tgz" => "TGZ",
                    "bz2" | "tbz2" => "TBZ",
                    "xz" | "txz" => "TXZ",
                    _ => continue,
                };
                decorations.push(json!({
                    "name": name,
                    "badge": {"text": label, "color": "neutral", "tooltip": crate::i18n::t("压缩包")}
                }));
            }
        }
        Ok(decorations)
    }
}
impl sdk::ContextMenuProvider for Archive {
    fn resolve(&self, ctx: &sdk::RequestContext) -> Result<Vec<Value>, RpcError> {
        serde_json::from_value(archive_menu(ctx.params())["items"].clone())
            .map_err(|error| RpcError::internal(error.to_string()))
    }
    fn execute(&self, ctx: &sdk::RequestContext) -> Result<Value, RpcError> {
        let mut params = ctx.params().clone();
        let operation = {
            let item = params["itemId"]
                .as_str()
                .ok_or_else(|| RpcError::invalid_params("missing menu itemId"))?;
            if item == "archive.advanced" {
                return Ok(json!({
                    "kind": "panel", "entry": "ui/dist/index.html",
                    "panelId": item, "title": crate::i18n::t("高级压缩"),
                    "size": {"width":400,"height":420}
                }));
            }
            let format = match item {
                "archive.zip" => "zip",
                "archive.rar" => "rar",
                "archive.7z" => "7z",
                "archive.tar" => "tar",
                "archive.tar.gz" => "tar.gz",
                "archive.tar.bz2" => "tar.bz2",
                "archive.tar.xz" => "tar.xz",
                "archive.extract" => "",
                _ => return Err(RpcError::invalid_params("unknown menu action")),
            };
            if !format.is_empty() {
                params["format"] = json!(format);
                "archive.compress"
            } else {
                params["path"] = json!(
                    ctx.target()
                        .ok_or_else(|| RpcError::invalid_params("missing target"))?
                        .reference
                );
                "archive.extract"
            }
        };
        run_operation(ctx, params, operation)
    }
}
impl sdk::HookProvider for Archive {
    fn invoke(&self, ctx: &sdk::RequestContext) -> Result<Value, RpcError> {
        let p = ctx.params();
        let host = ctx;

        if p.get("event").and_then(Value::as_str) != Some("beforeOpen") {
            return Ok(json!({ "handled": false }));
        }
        before_open(&resources::input(host, "path")?.reference, p, host)
    }
}
