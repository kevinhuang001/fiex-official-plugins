//! Standalone WebDAV adapter. The Fiex host owns mount configuration,
//! credential storage, cache, sync rules and conflict resolution.
use fiex_plugin_sdk::{self as sdk, RpcError};
use futures_util::StreamExt;
use quick_xml::events::Event;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    time::{Duration, UNIX_EPOCH},
};
use tokio::runtime::Builder;

const DAV_BODY: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:"><d:prop><d:displayname/><d:resourcetype/><d:getcontentlength/><d:getlastmodified/><d:getcontenttype/></d:prop></d:propfind>"#;
#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredConnection {
    url: String,
    #[serde(default)]
    username: String,
    #[serde(default)]
    password: String,
    #[serde(default)]
    allow_invalid_certs: bool,
    #[serde(default)]
    network_domains: Vec<String>,
}
struct RemoteFile {
    name: String,
    path: String,
    is_dir: bool,
    size: u64,
    modified: u64,
}

fn trim_host(value: &str) -> String {
    value
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase()
}
fn is_loopback(host: &str) -> bool {
    let host = trim_host(host);
    host == "localhost"
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

fn safe_relative(value: &str) -> Result<(), RpcError> {
    if value.starts_with('/')
        || value
            .split('/')
            .any(|p| p == "." || p == ".." || p.contains(['\\', '\0']))
    {
        return Err(RpcError::invalid_params("invalid node ID"));
    }
    Ok(())
}
struct ReadStream {
    connection: StoredConnection,
    node_id: String,
}
type Streams = Rc<RefCell<HashMap<String, ReadStream>>>;

fn dispatch(
    method: &str,
    p: Value,
    streams: &Streams,
    next_stream: &Cell<u64>,
) -> Result<Value, RpcError> {
    if !matches!(
        method,
        "connection.validate"
            | "fs.list"
            | "fs.readSmall"
            | "fs.openRead"
            | "fs.readStream"
            | "fs.closeRead"
    ) {
        return Err(RpcError::method_not_found(method));
    }
    // Stream reads are bound to an already-opened handle and never resend
    // credentials. Only the methods that initiate a connection parse it.
    let streaming = matches!(method, "fs.readStream" | "fs.closeRead");
    let cfg: StoredConnection = if streaming {
        StoredConnection::default()
    } else {
        serde_json::from_value(
            p.get("connection")
                .cloned()
                .ok_or_else(|| RpcError::invalid_params("missing connection"))?,
        )
        .map_err(|e| RpcError::invalid_params(e.to_string()))?
    };
    if !streaming && !(cfg.url.starts_with("https://") || cfg.url.starts_with("http://")) {
        return Err(RpcError::invalid_params("invalid WebDAV URL"));
    }
    if !streaming {
        let target = reqwest::Url::parse(&cfg.url)
            .ok()
            .and_then(|url| url.host_str().map(trim_host))
            .ok_or_else(|| RpcError::invalid_params("invalid WebDAV host"))?;
        if !is_loopback(&target)
            && !cfg
                .network_domains
                .iter()
                .any(|domain| trim_host(domain) == target)
        {
            return Err(RpcError::invalid_params("network domain not granted"));
        }
    }
    let path = p
        .get("directoryId")
        .or_else(|| p.get("nodeId"))
        .and_then(Value::as_str)
        .unwrap_or("");
    safe_relative(path)?;
    let rt = Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| RpcError::internal(e.to_string()))?;
    match method {
        "connection.validate" => {
            rt.block_on(webdav_list(&cfg, ""))
                .map_err(RpcError::internal)?;
            Ok(json!({"valid":true}))
        }
        "fs.list" => {
            let mut nodes = rt
                .block_on(webdav_list(&cfg, path))
                .map_err(RpcError::internal)?;
            nodes.sort_by(|a, b| a.name.cmp(&b.name));
            let start = p
                .get("cursor")
                .and_then(Value::as_str)
                .unwrap_or("0")
                .parse::<usize>()
                .map_err(|_| RpcError::invalid_params("invalid cursor"))?;
            if start > nodes.len() {
                return Err(RpcError::invalid_params("cursor out of range"));
            }
            let limit = p
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(200)
                .clamp(1, 500) as usize;
            let end = (start + limit).min(nodes.len());
            let entries = nodes[start..end]
                .iter()
                .map(|f| {
                    let id = if path.is_empty() {
                        f.path.clone()
                    } else {
                        format!("{}/{}", path.trim_matches('/'), f.path)
                    };
                    json!({"nodeId":id,"name":f.name,"kind":if f.is_dir{"directory"}else{"file"},
                    "size":f.size,"modified":f.modified})
                })
                .collect::<Vec<_>>();
            Ok(
                json!({"entries":entries,"nextCursor":if end<nodes.len(){Some(end.to_string())}else{None},
                "snapshotToken":null,"identityStability":"path"}),
            )
        }
        "fs.readSmall" => {
            let max = p
                .get("maxBytes")
                .and_then(Value::as_u64)
                .unwrap_or(262144)
                .clamp(1, 1048576);
            let data = rt
                .block_on(async {
                    let req = auth(
                        webdav_client(&cfg)?
                            .get(webdav_url(&cfg, path))
                            .header("Range", format!("bytes=0-{}", max - 1)),
                        &cfg,
                    );
                    let res = req
                        .send()
                        .await
                        .map_err(|e| e.to_string())?
                        .error_for_status()
                        .map_err(|e| e.to_string())?;
                    if res.status().as_u16() != 206
                        && res.content_length().unwrap_or(u64::MAX) > max
                    {
                        return Err("server ignored Range".into());
                    }
                    let mut data = Vec::with_capacity((max.min(65536)) as usize);
                    let mut stream = res.bytes_stream();
                    while let Some(chunk) = stream.next().await {
                        let chunk = chunk.map_err(|e| e.to_string())?;
                        if data.len().saturating_add(chunk.len()) > max as usize {
                            return Err("file exceeds preview limit".into());
                        }
                        data.extend_from_slice(&chunk);
                    }
                    Ok::<_, String>(data)
                })
                .map_err(RpcError::internal)?;
            use base64::Engine;
            Ok(
                json!({"base64":base64::engine::general_purpose::STANDARD.encode(&data),"length":data.len()}),
            )
        }
        "fs.openRead" => {
            if path.is_empty() {
                return Err(RpcError::invalid_params("missing node ID"));
            }
            let stream_id = format!("stream-{}", next_stream.get() + 1);
            next_stream.set(next_stream.get() + 1);
            streams.borrow_mut().insert(
                stream_id.clone(),
                ReadStream {
                    connection: cfg,
                    node_id: path.to_string(),
                },
            );
            Ok(json!({"streamId": stream_id}))
        }
        "fs.readStream" => {
            let stream_id = p
                .get("streamId")
                .and_then(Value::as_str)
                .ok_or_else(|| RpcError::invalid_params("missing streamId"))?;
            let offset = p
                .get("offset")
                .and_then(Value::as_u64)
                .ok_or_else(|| RpcError::invalid_params("missing offset"))?;
            let max = p
                .get("maxBytes")
                .and_then(Value::as_u64)
                .ok_or_else(|| RpcError::invalid_params("missing maxBytes"))?;
            if max == 0 || max > 256 * 1024 {
                return Err(RpcError::invalid_params("invalid chunk size"));
            }
            let streams = streams.borrow();
            let stream = streams
                .get(stream_id)
                .ok_or_else(|| RpcError::invalid_params("unknown read stream"))?;
            let (bytes, eof) = rt
                .block_on(webdav_read_chunk(
                    &stream.connection,
                    &stream.node_id,
                    offset,
                    max,
                ))
                .map_err(RpcError::internal)?;
            use base64::Engine;
            Ok(
                json!({"base64":base64::engine::general_purpose::STANDARD.encode(&bytes),
                "bytesRead":bytes.len(),"eof":eof}),
            )
        }
        "fs.closeRead" => {
            let stream_id = p
                .get("streamId")
                .and_then(Value::as_str)
                .ok_or_else(|| RpcError::invalid_params("missing streamId"))?;
            streams.borrow_mut().remove(stream_id);
            Ok(Value::Null)
        }
        _ => Err(RpcError::method_not_found(method)),
    }
}
fn main() {
    let streams: Streams = Rc::new(RefCell::new(HashMap::new()));
    let next_stream = Cell::new(0u64);
    if let Err(e) = sdk::serve("org.fiex.webdav", move |method, params| {
        dispatch(method, params, &streams, &next_stream)
    }) {
        eprintln!("WebDAV plugin IPC failed: {e}");
        std::process::exit(1);
    }
}

fn webdav_client(conn: &StoredConnection) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .danger_accept_invalid_certs(conn.allow_invalid_certs)
        .timeout(Duration::from_secs(90))
        .build()
        .map_err(|e| e.to_string())
}

fn webdav_url(conn: &StoredConnection, rel: &str) -> String {
    let base = conn.url.trim_end_matches('/');
    if rel.is_empty() {
        format!("{base}/")
    } else {
        let path = rel
            .split('/')
            .filter(|part| !part.is_empty())
            .map(|part| urlencoding::encode(part).into_owned())
            .collect::<Vec<_>>()
            .join("/");
        format!("{base}/{path}")
    }
}

fn auth(req: reqwest::RequestBuilder, conn: &StoredConnection) -> reqwest::RequestBuilder {
    if conn.username.is_empty() {
        req
    } else {
        req.basic_auth(&conn.username, Some(&conn.password))
    }
}

async fn webdav_list(conn: &StoredConnection, rel: &str) -> Result<Vec<RemoteFile>, String> {
    let client = webdav_client(conn)?;
    let url = format!("{}/", webdav_url(conn, rel).trim_end_matches('/'));
    let request = client
        .request(
            reqwest::Method::from_bytes(b"PROPFIND").map_err(|e| e.to_string())?,
            &url,
        )
        .header("Depth", "1")
        .header("Content-Type", "application/xml; charset=utf-8")
        .body(DAV_BODY);
    let response = auth(request, conn)
        .send()
        .await
        .map_err(|e| format!("WebDAV 连接失败：{e}"))?;
    if !response.status().is_success() && response.status().as_u16() != 207 {
        return Err(format!("WebDAV 返回 {}", response.status()));
    }
    let xml = response.text().await.map_err(|e| e.to_string())?;
    parse_dav(&url, rel, &xml)
}

fn decode_href(href: &str) -> String {
    urlencoding::decode(href)
        .map(|value| value.into_owned())
        .unwrap_or_else(|_| href.into())
}

fn parse_dav(url: &str, rel: &str, xml: &str) -> Result<Vec<RemoteFile>, String> {
    let base = reqwest::Url::parse(url).map_err(|e| e.to_string())?;
    let base_path = decode_href(base.path());
    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut current: Option<RemoteFile> = None;
    let mut field: Option<&str> = None;
    let mut text = String::new();
    let mut out = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) => {
                let name = event.local_name();
                match name.as_ref() {
                    b"response" => {
                        current = Some(RemoteFile {
                            name: String::new(),
                            path: String::new(),
                            is_dir: false,
                            size: 0,
                            modified: 0,
                        })
                    }
                    b"href" => field = Some("href"),
                    b"displayname" => field = Some("displayname"),
                    b"getcontentlength" => field = Some("size"),
                    b"getlastmodified" => field = Some("modified"),
                    _ => {}
                }
                text.clear();
            }
            Ok(Event::Empty(event)) => {
                if event.local_name().as_ref() == b"collection" {
                    if let Some(item) = current.as_mut() {
                        item.is_dir = true;
                    }
                }
            }
            Ok(Event::Text(event)) => {
                if field.is_some() {
                    text.push_str(&event.unescape().unwrap_or_default());
                }
            }
            Ok(Event::End(event)) => {
                if let Some(item) = current.as_mut() {
                    match field {
                        Some("href") => {
                            if let Ok(full) = base.join(text.trim()) {
                                let path = decode_href(full.path());
                                if let Some(relative) = path.strip_prefix(&base_path) {
                                    let relative = relative.trim_matches('/');
                                    if !relative.is_empty() {
                                        item.path = relative.into();
                                        item.name =
                                            relative.rsplit('/').next().unwrap_or(relative).into();
                                    }
                                }
                            }
                        }
                        Some("displayname") => {
                            if !text.trim().is_empty() {
                                item.name = text.trim().into();
                            }
                        }
                        Some("size") => item.size = text.trim().parse().unwrap_or(0),
                        Some("modified") => {
                            if let Ok(time) = httpdate::parse_http_date(text.trim()) {
                                item.modified = time
                                    .duration_since(UNIX_EPOCH)
                                    .map(|d| d.as_secs())
                                    .unwrap_or_default();
                            }
                        }
                        _ => {}
                    }
                }
                field = None;
                text.clear();
                if event.local_name().as_ref() == b"response" {
                    if let Some(mut item) = current.take() {
                        let requested = rel.trim_matches('/');
                        if !item.path.is_empty() && item.path.trim_matches('/') != requested {
                            if item.is_dir {
                                item.size = 0;
                            }
                            out.push(item);
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("WebDAV 响应解析失败：{e}")),
            _ => {}
        }
    }
    Ok(out)
}

/// One range request is bounded to one IPC frame. No host filesystem
/// pathname is ever accepted from the plugin caller.
async fn webdav_read_chunk(
    conn: &StoredConnection,
    rel: &str,
    offset: u64,
    max: u64,
) -> Result<(Vec<u8>, bool), String> {
    let request = auth(
        webdav_client(conn)?
            .get(webdav_url(conn, rel))
            .header("Range", format!("bytes={offset}-{}", offset + max - 1)),
        conn,
    );
    let response = request.send().await.map_err(|e| e.to_string())?;
    if response.status() == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
        return Ok((Vec::new(), true));
    }
    let partial = response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    if !partial && !(offset == 0 && response.status().is_success()) {
        return Err("WebDAV server did not respect byte range".into());
    }
    let eof = if partial {
        let header = response
            .headers()
            .get(reqwest::header::CONTENT_RANGE)
            .ok_or("missing Content-Range")?
            .to_str()
            .map_err(|e| e.to_string())?;
        let (range, total) = header.split_once('/').ok_or("invalid Content-Range")?;
        let (start, end) = range
            .strip_prefix("bytes ")
            .ok_or("invalid Content-Range unit")?
            .split_once('-')
            .ok_or("invalid Content-Range bounds")?;
        let start: u64 = start.parse().map_err(|_| "invalid range offset")?;
        let end: u64 = end.parse().map_err(|_| "invalid range end")?;
        let total: u64 = total.parse().map_err(|_| "unknown total length")?;
        if start != offset || end < start || end - start + 1 > max || end >= total {
            return Err("WebDAV server returned an unexpected range".into());
        }
        end + 1 == total
    } else {
        true
    };
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let part = chunk.map_err(|e| e.to_string())?;
        if bytes.len().saturating_add(part.len()) > max as usize {
            return Err("WebDAV response exceeds requested chunk".into());
        }
        bytes.extend_from_slice(&part);
    }
    Ok((bytes, eof))
}
