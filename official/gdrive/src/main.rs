//! Google Drive v3 adapter. All HTTP goes through Core's authorized network
//! transport; the plugin has no filesystem, subprocess or socket permission.
mod reads;
mod writes;
use fiex_plugin_sdk::{self as sdk, RpcError};
use serde_json::{json, Value};
use std::sync::Mutex;

const API: &str = "https://www.googleapis.com/drive/v3";
fn error(e: impl std::fmt::Display) -> RpcError {
    RpcError::internal(e.to_string())
}
fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str, RpcError> {
    v[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| RpcError::invalid_params(format!("missing {key}")))
}
fn node(v: &str) -> Result<&str, RpcError> {
    if v.is_empty()
        || v.len() > 256
        || !v
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
    {
        return Err(RpcError::invalid_params("invalid Drive file ID"));
    }
    Ok(v)
}
fn token(ctx: &sdk::RequestContext, cfg: &Value) -> Result<String, RpcError> {
    use std::{
        collections::HashMap,
        sync::OnceLock,
        time::{Duration, Instant},
    };
    type Tokens = HashMap<String, (String, Instant)>;
    static CACHE: OnceLock<Mutex<Tokens>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let key = serde_json::to_string(cfg).map_err(error)?;
    if let Some((token, until)) = cache.lock().map_err(error)?.get(&key) {
        if *until > Instant::now() {
            return Ok(token.clone());
        }
    }

    let body = format!(
        "client_id={}&client_secret={}&refresh_token={}&grant_type=refresh_token",
        urlencoding::encode(text(cfg, "clientId")?),
        urlencoding::encode(text(cfg, "clientSecret")?),
        urlencoding::encode(text(cfg, "refreshToken")?)
    );
    let value: Value = ctx
        .http()
        .post("https://oauth2.googleapis.com/token")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.json())
        .map_err(error)?;
    let token = text(&value, "access_token")?.to_string();
    let ttl = value["expires_in"]
        .as_u64()
        .unwrap_or(3600)
        .min(3600)
        .saturating_sub(60);
    let mut cached = cache.lock().map_err(error)?;
    if cached.len() >= 32 {
        cached.clear();
    }
    cached.insert(
        key,
        (token.clone(), Instant::now() + Duration::from_secs(ttl)),
    );
    Ok(token)
}
fn request(ctx: &sdk::RequestContext, token: &str, method: &str, url: &str) -> sdk::http::Request {
    ctx.http().request(method, url).bearer_auth(token)
}
fn metadata(ctx: &sdk::RequestContext, token: &str, id: &str) -> Result<(Value, String), RpcError> {
    let response = request(ctx, token, "GET", &format!("{API}/files/{}?fields=id,name,mimeType,size,version,trashed,capabilities(canEdit)&supportsAllDrives=true", node(id)?))
        .send().and_then(|r| r.error_for_status()).map_err(error)?;
    let etag = response.headers.get("etag").cloned().unwrap_or_default();
    let value: Value = response.json().map_err(error)?;
    if value["trashed"].as_bool() == Some(true) {
        return Err(error("Drive file was trashed; local copy retained"));
    }
    Ok((value, etag))
}
fn main() {
    let state = Mutex::new(writes::State::default());
    if let Err(e) = sdk::serve("org.fiex.gdrive", move |ctx| {
        let method = ctx.method();
        match method {
            "connection.validate"
            | "fs.list"
            | "fs.stat"
            | "fs.openRead"
            | "fs.readStream"
            | "fs.closeRead" => reads::dispatch(&ctx, &state),
            "fs.writeFile" => writes::write_file(&ctx),
            _ => Err(RpcError::method_not_found(method)),
        }
    }) {
        eprintln!("Drive IPC failed: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ids_are_not_paths_or_urls() {
        assert!(node("abc_123-Z").is_ok());
        for value in ["", "../secret", "https://evil", "a/b", "a' or true"] {
            assert!(node(value).is_err());
        }
    }
}
