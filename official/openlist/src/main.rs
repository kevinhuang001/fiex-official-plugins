//! OpenList (AList-compatible) native API adapter.
//!
//! Credentials live in the plugin's settings, not in the connection: one
//! account per plugin instance. Files are read through `/api/fs/get`, which
//! returns a storage URL; reads are capped because the whole payload is kept in
//! memory for the duration of one read stream.
use fiex_plugin_sdk::{self as sdk, RpcError};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Duration};

const MAX_READ: u64 = 32 * 1024 * 1024;

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Config {
    url: String,
    username: String,
    password: String,
    timeout: u64,
}

#[derive(Default)]
struct State {
    token: RefCell<Option<String>>,
    reads: RefCell<HashMap<String, Vec<u8>>>,
    next: RefCell<u64>,
}

fn stored(host: &mut dyn sdk::HostServices, key: &str) -> Option<Value> {
    host.storage_get("global", key).ok()
}

fn text(value: Option<Value>) -> String {
    value
        .as_ref()
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
}

fn config(host: &mut dyn sdk::HostServices) -> Result<Config, RpcError> {
    let url = text(stored(host, "url")).trim_end_matches('/').to_string();
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(RpcError::internal("请在插件设置中填写 http(s) 服务器地址"));
    }
    let timeout = stored(host, "timeout")
        .as_ref()
        .and_then(Value::as_u64)
        .unwrap_or(30)
        .clamp(5, 120);
    Ok(Config {
        url,
        username: text(stored(host, "username")),
        password: host
            .secret_get("global", "password")
            .unwrap_or_default()
            .trim()
            .to_string(),
        timeout,
    })
}

fn post(
    host: &mut dyn sdk::HostServices,
    cfg: &Config,
    token: Option<&str>,
    endpoint: &str,
    body: &Value,
) -> Result<Value, RpcError> {
    let client = host.http();
    let mut request = client
        .request("POST", &format!("{}{endpoint}", cfg.url))
        .timeout(Duration::from_secs(cfg.timeout))
        .header("Content-Type", "application/json");
    if let Some(token) = token.filter(|value| !value.is_empty()) {
        request = request.header("Authorization", token);
    }
    let response = request
        .body(body.to_string())
        .send()
        .map_err(|error| RpcError::internal(format!("OpenList 请求失败: {error}")))?;
    if response.status >= 400 {
        return Err(RpcError::internal(format!(
            "OpenList HTTP {}",
            response.status
        )));
    }
    let text = response
        .text()
        .map_err(|error| RpcError::internal(format!("OpenList 读取响应失败: {error}")))?;
    let parsed: Value = serde_json::from_str(&text)
        .map_err(|error| RpcError::internal(format!("OpenList 返回非 JSON: {error}")))?;
    let code = parsed.get("code").and_then(Value::as_i64).unwrap_or(0);
    if code != 200 {
        let message = parsed
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("未知错误");
        return Err(RpcError::internal(format!("OpenList: {message}")));
    }
    Ok(parsed)
}

fn login(host: &mut dyn sdk::HostServices, cfg: &Config) -> Result<String, RpcError> {
    let parsed = post(
        host,
        cfg,
        None,
        "/api/auth/login",
        &json!({ "username": cfg.username, "password": cfg.password }),
    )?;
    parsed
        .pointer("/data/token")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| RpcError::internal("登录失败：服务器未返回 token"))
}

fn token(
    host: &mut dyn sdk::HostServices,
    cfg: &Config,
    state: &State,
) -> Result<String, RpcError> {
    if let Some(value) = state.token.borrow().clone() {
        return Ok(value);
    }
    let value = login(host, cfg)?;
    *state.token.borrow_mut() = Some(value.clone());
    Ok(value)
}

fn authorized(
    host: &mut dyn sdk::HostServices,
    cfg: &Config,
    state: &State,
    endpoint: &str,
    body: &Value,
) -> Result<Value, RpcError> {
    let current = token(host, cfg, state)?;
    match post(host, cfg, Some(&current), endpoint, body) {
        // A stale token is the common failure after a server restart: log in
        // once more before surfacing the error.
        Err(error) if error.message.contains("401") => {
            *state.token.borrow_mut() = None;
            let fresh = token(host, cfg, state)?;
            post(host, cfg, Some(&fresh), endpoint, body)
        }
        other => other,
    }
}

fn api_path(relative: &str) -> String {
    if relative.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", relative.trim_matches('/'))
    }
}

fn safe_relative(value: &str) -> Result<(), RpcError> {
    if value.starts_with('/')
        || value
            .split('/')
            .any(|part| part == "." || part == ".." || part.contains(['\\', '\0']))
    {
        return Err(RpcError::invalid_params("invalid node ID"));
    }
    Ok(())
}

fn join(relative: &str, name: &str) -> String {
    if relative.is_empty() {
        name.to_string()
    } else {
        format!("{}/{}", relative.trim_end_matches('/'), name)
    }
}

fn raw_url(
    host: &mut dyn sdk::HostServices,
    cfg: &Config,
    state: &State,
    relative: &str,
) -> Result<String, RpcError> {
    let parsed = authorized(
        host,
        cfg,
        state,
        "/api/fs/get",
        &json!({ "path": api_path(relative), "password": "" }),
    )?;
    parsed
        .pointer("/data/raw_url")
        .and_then(Value::as_str)
        .filter(|value| value.starts_with("http://") || value.starts_with("https://"))
        .map(str::to_string)
        .ok_or_else(|| RpcError::internal("服务器未返回下载地址"))
}

fn download(
    host: &mut dyn sdk::HostServices,
    cfg: &Config,
    url: &str,
) -> Result<(Vec<u8>, u64), RpcError> {
    let client = host.http();
    let response = client
        .request("GET", url)
        .timeout(Duration::from_secs(cfg.timeout.max(30)))
        .send()
        .map_err(|error| RpcError::internal(format!("下载失败: {error}")))?;
    let status = response.status;
    if status != 200 {
        return Err(RpcError::internal(format!("下载失败: HTTP {status}")));
    }
    let bytes = response
        .bytes()
        .map_err(|error| RpcError::internal(format!("下载失败: {error}")))?;
    Ok((bytes, status as u64))
}

/// RFC 3339 (openlist returns `2024-01-02T03:04:05Z`) to epoch seconds.
fn parse_modified(value: &str) -> u64 {
    if value.len() < 19 {
        return 0;
    }
    let number = |range: std::ops::Range<usize>| value.get(range)?.parse::<i64>().ok();
    let (Some(year), Some(month), Some(day), Some(hour), Some(minute), Some(second)) = (
        number(0..4),
        number(5..7),
        number(8..10),
        number(11..13),
        number(14..16),
        number(17..19),
    ) else {
        return 0;
    };
    let (year, month, day) = if month <= 2 {
        (year - 1, month + 12, day)
    } else {
        (year, month, day)
    };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    let mut timestamp = days * 86_400 + hour * 3_600 + minute * 60 + second;
    let suffix = &value[19..];
    let offset = suffix
        .strip_prefix(['+', '-'])
        .and_then(|rest| rest.split_once(':'))
        .and_then(|(hours, minutes)| {
            let hours = hours.parse::<i64>().ok()?;
            let minutes = minutes.parse::<i64>().ok()?;
            Some(hours * 3_600 + minutes * 60)
        });
    if let Some(offset) = offset {
        timestamp += if suffix.starts_with('-') {
            offset
        } else {
            -offset
        };
    }
    timestamp.max(0) as u64
}

fn dispatch(
    method: &str,
    p: Value,
    state: &State,
    host: &mut dyn sdk::HostServices,
) -> Result<Value, RpcError> {
    if method == "fs.closeRead" {
        let id = p.get("streamId").and_then(Value::as_str).unwrap_or("");
        state.reads.borrow_mut().remove(id);
        return Ok(Value::Null);
    }
    let cfg = config(host)?;
    match method {
        "connection.validate" | "fs.list" => {
            let directory = p
                .get("directoryId")
                .or_else(|| p.get("nodeId"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            safe_relative(&directory)?;
            let parsed = authorized(
                host,
                &cfg,
                state,
                "/api/fs/list",
                &json!({
                    "path": api_path(&directory),
                    "password": "",
                    "page": 1,
                    "per_page": 0,
                    "refresh": false
                }),
            )?;
            if method == "connection.validate" {
                return Ok(json!({ "valid": true }));
            }
            let mut entries = parsed
                .pointer("/data/content")
                .and_then(Value::as_array)
                .map(|items| items.as_slice())
                .unwrap_or_default()
                .iter()
                .filter_map(|item| {
                    let name = item.get("name").and_then(Value::as_str)?;
                    if name.is_empty() || name.contains(['/', '\\', '\0']) {
                        return None;
                    }
                    let is_dir = item.get("is_dir").and_then(Value::as_bool).unwrap_or(false);
                    Some(json!({
                        "nodeId": join(&directory, name),
                        "name": name,
                        "kind": if is_dir { "directory" } else { "file" },
                        "size": item.get("size").and_then(Value::as_u64).unwrap_or(0),
                        "modified": parse_modified(
                            item.get("modified").and_then(Value::as_str).unwrap_or(""),
                        ),
                    }))
                })
                .collect::<Vec<_>>();
            entries.sort_by(|a, b| {
                a["name"]
                    .as_str()
                    .unwrap_or("")
                    .cmp(b["name"].as_str().unwrap_or(""))
            });
            let start = p
                .get("cursor")
                .and_then(Value::as_str)
                .unwrap_or("0")
                .parse::<usize>()
                .map_err(|error| RpcError::invalid_params(error.to_string()))?;
            if start > entries.len() {
                return Err(RpcError::invalid_params("cursor out of range"));
            }
            let limit = p
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(200)
                .clamp(1, 500) as usize;
            let end = start.saturating_add(limit).min(entries.len());
            Ok(json!({
                "entries": entries[start..end].to_vec(),
                "nextCursor": if end < entries.len() { Some(end.to_string()) } else { None },
                "snapshotToken": null,
                "identityStability": "path"
            }))
        }
        "fs.readSmall" | "fs.openRead" => {
            let relative = p
                .get("nodeId")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            safe_relative(&relative)?;
            if relative.is_empty() {
                return Err(RpcError::invalid_params("missing node ID"));
            }
            let url = raw_url(host, &cfg, state, &relative)?;
            if method == "fs.openRead" {
                if state.reads.borrow().len() >= 8 {
                    return Err(RpcError::internal("too many open reads"));
                }
                let (bytes, _) = download(host, &cfg, &url)?;
                if bytes.len() as u64 > MAX_READ {
                    return Err(RpcError::internal("文件超过 32 MiB，暂不支持预览"));
                }
                *state.next.borrow_mut() += 1;
                let id = format!("openlist-read-{}", state.next.borrow());
                state.reads.borrow_mut().insert(id.clone(), bytes);
                return Ok(json!({ "streamId": id }));
            }
            let offset = p.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
            let max = p
                .get("maxBytes")
                .and_then(Value::as_u64)
                .unwrap_or(262144)
                .clamp(1, 1048576) as usize;
            let (bytes, _) = download(host, &cfg, &url)?;
            if offset > bytes.len() {
                return Err(RpcError::invalid_params("offset out of range"));
            }
            let end = offset.saturating_add(max).min(bytes.len());
            use base64::Engine;
            let slice = &bytes[offset..end];
            Ok(json!({
                "base64": base64::engine::general_purpose::STANDARD.encode(slice),
                "length": slice.len(),
                "bytesRead": slice.len(),
                "eof": end == bytes.len()
            }))
        }
        "fs.readStream" => {
            let id = p.get("streamId").and_then(Value::as_str).unwrap_or("");
            let offset = p.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
            let max = p
                .get("maxBytes")
                .and_then(Value::as_u64)
                .unwrap_or(262144)
                .clamp(1, 1048576) as usize;
            let bytes = state.reads.borrow();
            let data = bytes
                .get(id)
                .ok_or_else(|| RpcError::invalid_params("unknown stream"))?;
            if offset > data.len() {
                return Err(RpcError::invalid_params("offset out of range"));
            }
            let end = offset.saturating_add(max).min(data.len());
            use base64::Engine;
            let slice = &data[offset..end];
            Ok(json!({
                "base64": base64::engine::general_purpose::STANDARD.encode(slice),
                "length": slice.len(),
                "bytesRead": slice.len(),
                "eof": end == data.len()
            }))
        }
        _ => Err(RpcError::method_not_found(method)),
    }
}

fn main() {
    let state = Rc::new(State::default());
    if let Err(error) = sdk::serve_with_host("org.fiex.openlist", move |method, params, host| {
        dispatch(method, params, &state, host)
    }) {
        eprintln!("OpenList IPC failed: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_joined_and_escaped_consistently() {
        assert_eq!(api_path(""), "/");
        assert_eq!(api_path("a/b"), "/a/b");
        assert_eq!(join("", "a"), "a");
        assert_eq!(join("a/", "b"), "a/b");
        assert!(safe_relative("../etc").is_err());
        assert!(safe_relative("/abs").is_err());
        assert!(safe_relative("a/../b").is_err());
        assert!(safe_relative("a/b").is_ok());
    }

    #[test]
    fn parses_modified_timestamps() {
        assert_eq!(parse_modified("1970-01-01T00:00:00Z"), 0);
        assert_eq!(parse_modified("2024-01-02T03:04:05Z"), 1704164645);
        assert_eq!(parse_modified("2024-01-02T04:04:05+01:00"), 1704164645);
        assert_eq!(parse_modified("nonsense"), 0);
    }

    #[test]
    fn stored_settings_are_typed() {
        assert_eq!(text(Some(json!("  x  "))), "x");
        assert_eq!(text(Some(json!(3))), "");
        assert_eq!(text(None), "");
    }
}
