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
fn archive_menu(params: &Value) -> Value {
    let names = params["selectionNames"].as_array();
    let selection = names.map_or(0, Vec::len);
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
        && names
            .and_then(|names| names.first())
            .and_then(Value::as_str)
            .is_some_and(archive);
    if only_archive {
        return json!({"items":[
            {"id":"archive.extract","title":"解压缩","kind":"action"}
        ]});
    }
    let formats = [
        ("archive.zip", "ZIP"),
        ("archive.rar", "RAR"),
        ("archive.7z", "7z"),
        ("archive.tar", "TAR"),
        ("archive.tar.gz", "TAR.GZ"),
        ("archive.tar.bz2", "TAR.BZ2"),
        ("archive.tar.xz", "TAR.XZ"),
        ("archive.advanced", "高级压缩…"),
    ];
    let mut items = vec![json!({
        "id":"archive.compress", "title":"压缩", "kind":"submenu"
    })];
    items.extend(formats.into_iter().map(|(id, title)| {
        json!({
            "id":id, "title":title, "kind":"action", "parentId":"archive.compress"
        })
    }));
    json!({"items":items})
}

fn context_name(params: &Value) -> Result<&str, RpcError> {
    params
        .pointer("/resource/metadata/name")
        .and_then(Value::as_str)
        .ok_or_else(|| RpcError::invalid_params("missing resource metadata name"))
}

fn archive_info(p: &Value, host: &sdk::RequestContext) -> Result<Value, RpcError> {
    let path = param(p, "path")?;
    let password = p.get("password").and_then(Value::as_str);
    let listing = listing::list(path, password, host).map_err(RpcError::internal)?;
    let compressed = host
        .files()
        .metadata(path)
        .map(|entry| entry.size)
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
    let format = path
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_uppercase())
        .unwrap_or_else(|| "UNKNOWN".into());
    let mut rows = vec![
        json!({"label":"格式","value":format}),
        json!({"label":"压缩后大小","value":format_size(compressed)}),
        json!({"label":"解压后大小","value":format_size(uncompressed)}),
        json!({"label":"文件数","value":count.to_string()}),
    ];
    if uncompressed > 0 {
        let saved = 1.0 - compressed as f64 / uncompressed as f64;
        rows.push(json!({"label":"压缩率","value":format!("{:.1}%", saved * 100.0)}));
    }
    if listing
        .get("encrypted")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        rows.push(json!({"label":"加密","value":"是"}));
    }
    if listing.get("supported").and_then(Value::as_bool) == Some(false) {
        if let Some(message) = listing.get("message").and_then(Value::as_str) {
            rows.push(json!({"label":"说明","value":message}));
        }
    }
    Ok(json!({"rows": rows}))
}

fn run(ctx: sdk::RequestContext) -> Result<Value, RpcError> {
    let method = ctx.method();
    let mut params = ctx.params().clone();
    if method == "decorations.resolve" {
        let mut decorations = Vec::new();
        if let Some(entries) = params.get("entries").and_then(Value::as_array) {
            for entry in entries {
                let Some(path) = entry.get("path").and_then(Value::as_str) else {
                    continue;
                };
                if entry.get("isDir").and_then(Value::as_bool) == Some(true) {
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
                    "path": path,
                    "badge": {"text": label, "color": "neutral", "tooltip": "压缩包"}
                }));
            }
        }
        return Ok(json!({"decorations": decorations}));
    }
    if method == "info.resolve" {
        let name = context_name(&params)?.to_owned();
        let rows = archive_info(&json!({"path": name}), &ctx)?;
        return Ok(
            json!({"sections":[{"id":"archive","title":"压缩包","rows":rows["rows"].clone()}]}),
        );
    }
    if method == "preview.open" {
        return Ok(json!({
            "kind": "html",
            "view": {"source":"package","entry":"ui/preview.html"}
        }));
    }
    if method == "contextMenu.resolve" {
        return Ok(archive_menu(&params));
    }
    let operation = if method == "contextMenu.execute" {
        let item = params["itemId"]
            .as_str()
            .ok_or_else(|| RpcError::invalid_params("missing menu itemId"))?;
        if item == "archive.advanced" {
            return Ok(json!({
                "kind": "panel", "entry": "ui/dist/index.html",
                "panelId": item, "title": "高级压缩",
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
            let selected = params["paths"]
                .as_array()
                .and_then(|paths| paths.first())
                .and_then(Value::as_str)
                .ok_or_else(|| RpcError::invalid_params("missing selection"))?
                .to_owned();
            params["path"] = json!(selected);
            "archive.extract"
        }
    } else {
        method
    };
    let p = &params;
    let host = &ctx;
    match operation {
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
        "archive.info" => archive_info(p, host),
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
