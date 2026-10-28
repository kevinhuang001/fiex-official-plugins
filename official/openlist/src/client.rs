//! OpenList authentication and JSON-over-HTTP plumbing.
//!
//! Every credential travels with the connection that owns the server address,
//! so one profile can reach several servers. Bearer tokens are cached per
//! (server, account) and refreshed once when the server reports them stale.
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

/// Cached login token, keyed by server and account.
#[derive(Default)]
pub(crate) struct Session {
    token: Mutex<Option<(String, String)>>,
}

fn text(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
}

fn number(value: Option<&Value>, fallback: u64) -> u64 {
    value
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))
        })
        .unwrap_or(fallback)
        .clamp(5, 120)
}

/// The connection owns the server address, the account and the timeout, so
/// Core approves exactly the resources that connection was created for.
pub(crate) fn config(connection: Option<&Value>) -> Result<Config, RpcError> {
    let connection = connection.ok_or_else(|| RpcError::internal("缺少连接配置"))?;
    let url = text(connection.get("url"))
        .trim_end_matches('/')
        .to_string();
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(RpcError::internal("请在连接设置中填写 http(s) 服务器地址"));
    }
    Ok(Config {
        url,
        username: text(connection.get("username")),
        password: connection
            .get("password")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        timeout: number(connection.get("timeout"), 30),
    })
}

fn session_key(cfg: &Config) -> String {
    format!("{}\n{}", cfg.url, cfg.username)
}

pub(crate) fn post_http(
    http: &sdk::HttpClient,
    cfg: &Config,
    token: Option<&str>,
    endpoint: &str,
    body: &Value,
) -> Result<Value, RpcError> {
    let mut request = http
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

fn post(
    host: &sdk::RequestContext,
    cfg: &Config,
    token: Option<&str>,
    endpoint: &str,
    body: &Value,
) -> Result<Value, RpcError> {
    post_http(&host.http(), cfg, token, endpoint, body)
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
    let key = session_key(cfg);
    if let Some((cached, value)) = session
        .token
        .lock()
        .map_err(|_| RpcError::internal("session cache unavailable"))?
        .as_ref()
    {
        if *cached == key {
            return Ok(value.clone());
        }
    }
    let value = login(host, cfg)?;
    *session
        .token
        .lock()
        .map_err(|_| RpcError::internal("session cache unavailable"))? = Some((key, value.clone()));
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

pub(crate) fn current_user(
    host: &sdk::RequestContext,
    cfg: &Config,
    session: &Session,
) -> Result<Value, RpcError> {
    let token = token(host, cfg, session)?;
    let response = host
        .http()
        .get(&format!("{}/api/me", cfg.url))
        .header("Authorization", token)
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.json::<Value>())
        .map_err(|e| RpcError::internal(e.to_string()))?;
    if response["code"].as_i64() != Some(200) {
        return Err(RpcError::internal("OpenList permission discovery failed"));
    }
    Ok(response["data"].clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_fields_are_typed() {
        assert_eq!(text(Some(&json!("  x  "))), "x");
        assert_eq!(text(Some(&json!(3))), "");
        assert_eq!(text(None), "");
        assert_eq!(number(Some(&json!(30)), 5), 30);
        assert_eq!(number(Some(&json!("45")), 5), 45);
        assert_eq!(number(Some(&json!("1")), 5), 5);
        assert_eq!(number(None, 30), 30);
    }

    #[test]
    fn session_key_separates_servers_and_accounts() {
        let first = Config {
            url: "https://a.example".into(),
            username: "one".into(),
            password: String::new(),
            timeout: 30,
        };
        let mut second = first.clone();
        assert_eq!(session_key(&first), session_key(&second));
        second.username = "two".into();
        assert_ne!(session_key(&first), session_key(&second));
    }
}
