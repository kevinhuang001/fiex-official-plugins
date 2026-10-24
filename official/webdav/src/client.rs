//! WebDAV HTTP and XML plumbing: request construction, URL escaping and
//! PROPFIND response parsing.
use fiex_plugin_sdk as sdk;
use quick_xml::events::Event;
use serde::Deserialize;
use std::time::{Duration, UNIX_EPOCH};

pub(crate) const DAV_BODY: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:"><d:prop><d:displayname/><d:resourcetype/><d:getcontentlength/><d:getlastmodified/><d:getcontenttype/><d:getetag/></d:prop></d:propfind>"#;

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoredConnection {
    pub(crate) url: String,
    #[serde(default)]
    pub(crate) username: String,
    #[serde(default)]
    pub(crate) password: String,
    #[serde(default)]
    pub(crate) allow_invalid_certs: bool,
}

pub(crate) struct RemoteFile {
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) is_dir: bool,
    pub(crate) size: u64,
    pub(crate) modified: u64,
    pub(crate) revision: String,
}

/// Build a request for `url` authenticated with the stored credentials.
pub(crate) fn request(
    client: &sdk::http::HttpClient,
    cfg: &StoredConnection,
    method: &str,
    url: &str,
) -> sdk::http::Request {
    client
        .request(method, url)
        .timeout(Duration::from_secs(90))
        .allow_invalid_certs(cfg.allow_invalid_certs)
        .basic_auth(&cfg.username, &cfg.password)
}

pub(crate) fn webdav_url(conn: &StoredConnection, rel: &str) -> String {
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

pub(crate) fn parse_dav(url: &str, rel: &str, xml: &str) -> Result<Vec<RemoteFile>, String> {
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
                            revision: String::new(),
                        })
                    }
                    b"href" => field = Some("href"),
                    b"displayname" => field = Some("displayname"),
                    b"getetag" => field = Some("revision"),
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
                        Some("revision") => item.revision = text.trim().into(),
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
