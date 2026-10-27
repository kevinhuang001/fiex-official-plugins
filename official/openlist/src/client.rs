//! OpenList authentication and JSON-over-HTTP plumbing.
//!
//! The account comes from the plugin settings while the connection owns the
//! server address. A single in-memory bearer token is cached per instance and
//! transparently refreshed once when the server reports it as stale.
use fiex_plugin_sdk::{self as sdk, RpcError};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{sync::Mutex, time::Duration};

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Config {
    pub(crate) url: String,
    username: String,
    password: String,
    timeout: u64,
}

/// Cached login token for one plugin instance.
#[derive(Default)]
pub(crate) struct Session {
    token: Mutex<Option<String>>,
}

fn stored(host: &sdk::RequestContext, key: &str) -> Option<Value> {
    host.storage().get::<Value>(key).ok().flatten()
}

fn text(value: Option<Value>) -> String {
    value
        .as_ref()
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
}

pub(crate) fn config(
    host: &sdk::RequestContext,
    connection: Option<&Value>,
) -> Result<Config, RpcError> {
    // The connection owns the server address so Core can approve exactly that
    // host; the account itself comes from the plugin settings.
    let from_connection = connection
        .and_then(|value| value.get("url"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let url = if from_connection.trim().is_empty() {
        text(stored(host, "url"))
    } else {
        from_connection.trim().to_string()
    }
    .trim_end_matches('/')
    .to_string();
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
            .secrets()
            .get::<String>("password")
            .map_err(|error| RpcError::internal(error.to_string()))?
            .unwrap_or_default()
            .trim()
            .to_string(),
        timeout,
    })
}

fn post(
    host: &sdk::RequestContext,
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

fn login(host: &sdk::RequestContext, cfg: &Config) -> Result<String, RpcError> {
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

pub(crate) fn token(
    host: &sdk::RequestContext,
    cfg: &Config,
    session: &Session,
) -> Result<String, RpcError> {
    if let Some(value) = session
        .token
        .lock()
        .map_err(|_| RpcError::internal("session cache unavailable"))?
        .clone()
    {
        return Ok(value);
    }
    let value = login(host, cfg)?;
    *session
        .token
        .lock()
        .map_err(|_| RpcError::internal("session cache unavailable"))? = Some(value.clone());
    Ok(value)
}

pub(crate) fn authorized(
    host: &sdk::RequestContext,
    cfg: &Config,
    session: &Session,
    endpoint: &str,
    body: &Value,
) -> Result<Value, RpcError> {
    let current = token(host, cfg, session)?;
    match post(host, cfg, Some(&current), endpoint, body) {
        // A stale token is the common failure after a server restart: log in
        // once more before surfacing the error.
        Err(error) if error.message.contains("401") => {
            *session
                .token
                .lock()
                .map_err(|_| RpcError::internal("session cache unavailable"))? = None;
            let fresh = token(host, cfg, session)?;
            post(host, cfg, Some(&fresh), endpoint, body)
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_settings_are_typed() {
        assert_eq!(text(Some(json!("  x  "))), "x");
        assert_eq!(text(Some(json!(3))), "");
        assert_eq!(text(None), "");
    }
}
