//! OpenList filesystem handlers: listing, small reads and read streams.
//!
//! Files are read through `/api/fs/get`, which returns a storage URL; reads are
//! capped because the whole payload is kept in memory for the duration of one
//! read stream.
use super::client::{self, Config, Session};
use fiex_plugin_sdk::{self as sdk, RpcError};
use serde_json::{json, Value};
use std::{cell::RefCell, collections::HashMap};

const MAX_READ: u64 = 32 * 1024 * 1024;

/// Buffered payloads backing `fs.openRead` streams.
#[derive(Default)]
pub(crate) struct Reads {
    data: RefCell<HashMap<String, Vec<u8>>>,
    next: RefCell<u64>,
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
    session: &Session,
    relative: &str,
) -> Result<String, RpcError> {
    let parsed = client::authorized(
        host,
        cfg,
        session,
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

pub(crate) fn list(
    method: &str,
    p: &Value,
    session: &Session,
    host: &mut dyn sdk::HostServices,
) -> Result<Value, RpcError> {
    let cfg = client::config(host, p.get("connection"))?;
    let directory = p
        .get("directoryId")
        .or_else(|| p.get("nodeId"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    safe_relative(&directory)?;
    let parsed = client::authorized(
        host,
        &cfg,
        session,
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

pub(crate) fn read(
    method: &str,
    p: &Value,
    session: &Session,
    reads: &Reads,
    host: &mut dyn sdk::HostServices,
) -> Result<Value, RpcError> {
    let cfg = client::config(host, p.get("connection"))?;
    let relative = p
        .get("nodeId")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    safe_relative(&relative)?;
    if relative.is_empty() {
        return Err(RpcError::invalid_params("missing node ID"));
    }
    let url = raw_url(host, &cfg, session, &relative)?;
    if method == "fs.openRead" {
        if reads.data.borrow().len() >= 8 {
            return Err(RpcError::internal("too many open reads"));
        }
        let (bytes, _) = client::download(host, &cfg, &url)?;
        if bytes.len() as u64 > MAX_READ {
            return Err(RpcError::internal("文件超过 32 MiB，暂不支持预览"));
        }
        *reads.next.borrow_mut() += 1;
        let id = format!("openlist-read-{}", reads.next.borrow());
        reads.data.borrow_mut().insert(id.clone(), bytes);
        return Ok(json!({ "streamId": id }));
    }
    let offset = p.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
    let max = p
        .get("maxBytes")
        .and_then(Value::as_u64)
        .unwrap_or(262144)
        .clamp(1, 1048576) as usize;
    let (bytes, _) = client::download(host, &cfg, &url)?;
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

pub(crate) fn read_stream(p: &Value, reads: &Reads) -> Result<Value, RpcError> {
    let id = p.get("streamId").and_then(Value::as_str).unwrap_or("");
    let offset = p.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
    let max = p
        .get("maxBytes")
        .and_then(Value::as_u64)
        .unwrap_or(262144)
        .clamp(1, 1048576) as usize;
    let buffers = reads.data.borrow();
    let data = buffers
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

pub(crate) fn close_read(p: &Value, reads: &Reads) {
    let id = p.get("streamId").and_then(Value::as_str).unwrap_or("");
    reads.data.borrow_mut().remove(id);
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
}
