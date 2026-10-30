//! OpenList filesystem handlers: listing, metadata and reads.
//!
//! Files are read through `/api/fs/get`, which returns a storage URL. The SDK
//! owns chunk offsets and stream ids, so a plain HTTP response is the reader.
use super::client::{self, Config, Session};
use fiex_plugin_sdk::{
    self as sdk,
    filesystem::{FsEntry, FsListing, FsRead, FsRequest},
    RpcError,
};
use serde_json::{json, Value};

pub(crate) fn api_path(relative: &str) -> String {
    if relative.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", relative.trim_matches('/'))
    }
}

pub(crate) fn safe_relative(value: &str) -> Result<(), RpcError> {
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

pub(crate) fn join_path(parent: &str, name: &str) -> String {
    join(parent, name)
}

pub(crate) fn split_path(node: &str) -> (&str, &str) {
    node.rsplit_once('/').unwrap_or(("", node))
}

pub(crate) fn valid_name(name: &str) -> Result<(), RpcError> {
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\', '\0']) {
        return Err(RpcError::invalid_params("invalid name"));
    }
    Ok(())
}

fn raw_url(
    host: &sdk::RequestContext,
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
        .ok_or_else(|| RpcError::internal(crate::i18n::t("服务器未返回下载地址")))
}

/// Revision string used for `ifRevision`; changes with content or metadata.
fn revision(data: &Value) -> String {
    json!([
        data["modified"],
        data["size"],
        data["hashinfo"],
        data["hash_info"]
    ])
    .to_string()
}

/// One `FsEntry` from the `data` object of `/api/fs/get`.
fn entry_from_data(data: &Value, node: &str) -> FsEntry {
    let is_dir = data["is_dir"].as_bool().unwrap_or(false);
    let name = data["name"]
        .as_str()
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            if node.is_empty() {
                "root".into()
            } else {
                node.rsplit('/').next().unwrap_or(node).into()
            }
        });
    let mut entry = if is_dir {
        FsEntry::directory(node, name)
    } else {
        FsEntry::file(node, name)
    };
    entry.size = Some(data["size"].as_u64().unwrap_or(0));
    entry.modified = Some(parse_modified(data["modified"].as_str().unwrap_or("")) as i64);
    entry.revision = Some(revision(data));
    entry
}

pub(crate) fn object(
    host: &sdk::RequestContext,
    cfg: &Config,
    session: &Session,
    node: &str,
) -> Result<Value, RpcError> {
    client::authorized(
        host,
        cfg,
        session,
        "/api/fs/get",
        &json!({ "path": api_path(node), "password": "" }),
    )
}

pub(crate) fn validate(host: &sdk::RequestContext, session: &Session) -> Result<Value, RpcError> {
    let cfg = client::config(host.params().get("connection"))?;
    client::authorized(
        host,
        &cfg,
        session,
        "/api/fs/list",
        &json!({
            "path": "/",
            "password": "",
            "page": 1,
            "per_page": 0,
            "refresh": false
        }),
    )?;
    Ok(json!({ "valid": true }))
}

pub(crate) fn list(request: &FsRequest<'_>, session: &Session) -> Result<FsListing, RpcError> {
    let host = request.ctx();
    let cfg = client::config(request.connection())?;
    let directory = request.node_id().unwrap_or("").to_owned();
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
            let node = join(&directory, name);
            let mut entry = if is_dir {
                FsEntry::directory(node, name)
            } else {
                FsEntry::file(node, name)
            };
            entry.size = Some(item.get("size").and_then(Value::as_u64).unwrap_or(0));
            entry.modified = Some(parse_modified(
                item.get("modified").and_then(Value::as_str).unwrap_or(""),
            ) as i64);
            Some(entry)
        })
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    let start = request
        .params()
        .get("cursor")
        .and_then(Value::as_str)
        .unwrap_or("0")
        .parse::<usize>()
        .map_err(|error| RpcError::invalid_params(error.to_string()))?;
    if start > entries.len() {
        return Err(RpcError::invalid_params("cursor out of range"));
    }
    let limit = request
        .params()
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(200)
        .clamp(1, 500) as usize;
    let total = entries.len();
    let end = start.saturating_add(limit).min(total);
    let next_cursor = (end < total).then(|| end.to_string());
    let entries = entries[start..end].to_vec();
    Ok(FsListing {
        entries,
        next_cursor,
        warnings: Vec::new(),
        truncated: false,
    })
}

pub(crate) fn stat(request: &FsRequest<'_>, session: &Session) -> Result<FsEntry, RpcError> {
    let host = request.ctx();
    let cfg = client::config(request.connection())?;
    let node = request.node_id().unwrap_or("");
    safe_relative(node)?;
    stat_node(host, &cfg, session, node)
}

pub(crate) fn stat_node(
    host: &sdk::RequestContext,
    cfg: &Config,
    session: &Session,
    node: &str,
) -> Result<FsEntry, RpcError> {
    let parsed = object(host, cfg, session, node)?;
    Ok(entry_from_data(&parsed["data"], node))
}

/// Stat with an already-authenticated token; used by the commit callback.
pub(crate) fn stat_http(
    http: &sdk::HttpClient,
    cfg: &Config,
    token: &str,
    node: &str,
) -> Result<FsEntry, RpcError> {
    let parsed = client::post_http(
        http,
        cfg,
        Some(token),
        "/api/fs/get",
        &json!({ "path": api_path(node), "password": "" }),
    )?;
    Ok(entry_from_data(&parsed["data"], node))
}

pub(crate) fn read(request: &FsRequest<'_>, session: &Session) -> Result<FsRead, RpcError> {
    let host = request.ctx();
    let cfg = client::config(request.connection())?;
    let relative = request.node_id().unwrap_or("").to_owned();
    safe_relative(&relative)?;
    if relative.is_empty() {
        return Err(RpcError::invalid_params("missing node ID"));
    }
    let url = raw_url(host, &cfg, session, &relative)?;
    if let Ok(reader) = host
        .aria2()
        .get(&url)
        .timeout(std::time::Duration::from_secs(90))
        .send()
    {
        return Ok(FsRead::new(reader, None));
    }
    let response = host
        .http()
        .get(&url)
        .send()
        .map_err(|e| RpcError::internal(e.to_string()))?;
    if response.status != 206 && response.status != 200 {
        return Err(RpcError::internal("storage did not honor range"));
    }
    Ok(FsRead::new(response, None))
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
