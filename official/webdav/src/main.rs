//! Standalone WebDAV adapter. The Fiex host owns mount configuration,
//! credential storage, cache, sync rules and conflict resolution.
use fiex_plugin_sdk::{self as sdk, RpcError};
use quick_xml::events::Event;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    io::Read,
    rc::Rc,
    time::{Duration, UNIX_EPOCH},
};

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
}
struct RemoteFile {
    name: String,
    path: String,
    is_dir: bool,
    size: u64,
    modified: u64,
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
    next: &Cell<u64>,
    host: &mut dyn sdk::HostServices,
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
    let id = p.get("streamId").and_then(Value::as_str).unwrap_or("");
    if method == "fs.closeRead" {
        streams.borrow_mut().remove(id);
        return Ok(Value::Null);
    }
    let (cfg, path) = if method == "fs.readStream" {
        let streams = streams.borrow();
        let stream = streams
            .get(id)
            .ok_or_else(|| RpcError::invalid_params("unknown stream"))?;
        (stream.connection.clone(), stream.node_id.clone())
    } else {
        let cfg: StoredConnection = serde_json::from_value(
            p.get("connection")
                .cloned()
                .ok_or_else(|| RpcError::invalid_params("missing connection"))?,
        )
        .map_err(|e| RpcError::invalid_params(e.to_string()))?;
        let path = p
            .get("directoryId")
            .or_else(|| p.get("nodeId"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        safe_relative(&path)?;
        (cfg, path)
    };
    let client = host.http();
    let request = |method: &str, url: &str| {
        client
            .request(method, url)
            .timeout(Duration::from_secs(90))
            .allow_invalid_certs(cfg.allow_invalid_certs)
            .basic_auth(&cfg.username, &cfg.password)
    };
    match method {
        "connection.validate" | "fs.list" => {
            let url = format!("{}/", webdav_url(&cfg, &path).trim_end_matches('/'));
            let xml = request("PROPFIND", &url)
                .header("Depth", "1")
                .header("Content-Type", "application/xml; charset=utf-8")
                .body(DAV_BODY)
                .send()
                .and_then(|r| r.error_for_status())
                .and_then(|r| r.text())
                .map_err(|e| RpcError::internal(e.to_string()))?;
            let mut nodes = parse_dav(&url, &path, &xml).map_err(RpcError::internal)?;
            nodes.sort_by(|a, b| a.name.cmp(&b.name));
            if method == "connection.validate" {
                return Ok(json!({"valid":true}));
            }
            let start = p
                .get("cursor")
                .and_then(Value::as_str)
                .unwrap_or("0")
                .parse::<usize>()
                .map_err(|e| RpcError::invalid_params(e.to_string()))?;
            if start > nodes.len() {
                return Err(RpcError::invalid_params("cursor out of range"));
            }
            let limit = p
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(200)
                .clamp(1, 500) as usize;
            let end = start.saturating_add(limit).min(nodes.len());
            let entries=nodes[start..end].iter().map(|f|json!({"nodeId":if path.is_empty(){f.path.clone()}else{format!("{}/{}",path.trim_matches('/'),f.path)},"name":f.name,"kind":if f.is_dir{"directory"}else{"file"},"size":f.size,"modified":f.modified})).collect::<Vec<_>>();
            Ok(
                json!({"entries":entries,"nextCursor":if end<nodes.len(){Some(end.to_string())}else{None},"snapshotToken":null,"identityStability":"path"}),
            )
        }
        "fs.openRead" => {
            if streams.borrow().len() >= 32 {
                return Err(RpcError::internal("too many read streams"));
            }
            next.set(next.get() + 1);
            let id = format!("dav-read-{}", next.get());
            streams.borrow_mut().insert(
                id.clone(),
                ReadStream {
                    connection: cfg,
                    node_id: path,
                },
            );
            Ok(json!({"streamId":id}))
        }
        _ => {
            let offset = p.get("offset").and_then(Value::as_u64).unwrap_or(0);
            let max = p
                .get("maxBytes")
                .and_then(Value::as_u64)
                .unwrap_or(262144)
                .clamp(1, 1048576);
            let end = offset
                .checked_add(max - 1)
                .ok_or_else(|| RpcError::invalid_params("range overflow"))?;
            let mut response = request("GET", &webdav_url(&cfg, &path))
                .header("Range", format!("bytes={offset}-{end}"))
                .send()
                .map_err(|e| RpcError::internal(e.to_string()))?;
            if response.status == 416 {
                return Ok(json!({"base64":"","bytesRead":0,"length":0,"eof":true}));
            }
            if response.status != 206 && !(offset == 0 && response.status == 200) {
                return Err(RpcError::internal("server did not respect byte range"));
            }
            let eof = if response.status == 206 {
                let header = response
                    .headers
                    .get("content-range")
                    .ok_or_else(|| RpcError::internal("missing Content-Range"))?;
                let (bounds, total) = header
                    .strip_prefix("bytes ")
                    .and_then(|v| v.split_once('/'))
                    .ok_or_else(|| RpcError::internal("invalid Content-Range"))?;
                let (start, last) = bounds
                    .split_once('-')
                    .ok_or_else(|| RpcError::internal("invalid Content-Range"))?;
                let start = start
                    .parse::<u64>()
                    .map_err(|e| RpcError::internal(e.to_string()))?;
                let last = last
                    .parse::<u64>()
                    .map_err(|e| RpcError::internal(e.to_string()))?;
                let total = total
                    .parse::<u64>()
                    .map_err(|e| RpcError::internal(e.to_string()))?;
                if start != offset || last < start || last - start >= max || last >= total {
                    return Err(RpcError::internal("unexpected Content-Range"));
                }
                last + 1 == total
            } else {
                true
            };
            let mut bytes = Vec::new();
            (&mut response)
                .take(max + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| RpcError::internal(e.to_string()))?;
            if bytes.len() as u64 > max {
                return Err(RpcError::internal("response exceeds requested range"));
            }
            use base64::Engine;
            Ok(
                json!({"base64":base64::engine::general_purpose::STANDARD.encode(&bytes),"length":bytes.len(),"bytesRead":bytes.len(),"eof":eof}),
            )
        }
    }
}
fn main() {
    let streams = Rc::new(RefCell::new(HashMap::new()));
    let next = Cell::new(0);
    if let Err(e) = sdk::serve_with_host("org.fiex.webdav", move |method, p, host| {
        dispatch(method, p, &streams, &next, host)
    }) {
        eprintln!("WebDAV IPC failed: {e}");
        std::process::exit(1);
    }
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

fn decode_href(href: &str) -> String {
    urlencoding::decode(href)
        .map(|value| value.into_owned())
        .unwrap_or_else(|_| href.into())
}

fn parse_dav(url: &str, rel: &str, xml: &str) -> Result<Vec<RemoteFile>, String> {
    let base = url::Url::parse(url).map_err(|e| e.to_string())?;
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
