//! User-owned Google Cloud Desktop OAuth client; no embedded shared credentials.
use crate::{error, http_request, text, API};
use fiex_plugin_sdk::{self as sdk, RpcError};
use serde_json::Value;
use std::sync::Mutex;

pub(super) fn token(http: &sdk::HttpClient, cfg: &Value) -> Result<String, RpcError> {
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
    let response = http
        .post("https://oauth2.googleapis.com/token")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .map_err(error)?;
    let status = response.status;
    let value: Value = response.json().map_err(error)?;
    if status >= 400 {
        if matches!(
            value["error"].as_str(),
            Some("invalid_grant" | "invalid_client")
        ) {
            let mut failure = error(crate::i18n::t(
                "Google 授权已过期或应用凭据已更改，请保存连接以重新授权",
            ));
            failure.data = Some(serde_json::json!({"reason":"reauthorization_required"}));
            return Err(failure);
        }
        return Err(error(sdk::i18n::format(
            crate::i18n::t("Google 令牌请求失败（HTTP {status}）"),
            &[("status", status.to_string())],
        )));
    }
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

fn browser_authorization(
    ctx: &sdk::RequestContext,
    cfg: &Value,
) -> Result<(String, String), RpcError> {
    let client_id = text(cfg, "clientId")?;
    let client_secret = text(cfg, "clientSecret")?;
    let mut request = sdk::OAuthRequest::new(
        "https://accounts.google.com/o/oauth2/v2/auth",
        client_id,
        vec!["https://www.googleapis.com/auth/drive".into()],
    );
    request
        .parameters
        .insert("access_type".into(), "offline".into());
    request.parameters.insert("prompt".into(), "consent".into());
    let code = ctx.oauth().authorize(&request)?;
    let body = format!("client_id={}&client_secret={}&code={}&redirect_uri={}&code_verifier={}&grant_type=authorization_code",
        urlencoding::encode(client_id), urlencoding::encode(client_secret),
        urlencoding::encode(&code.code), urlencoding::encode(&code.redirect_uri), urlencoding::encode(&code.code_verifier));
    let response = ctx
        .http()
        .post("https://oauth2.googleapis.com/token")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .map_err(error)?;
    let status = response.status;
    let tokens: Value = response.json().map_err(error)?;
    if status >= 400 {
        return Err(error(sdk::i18n::format(crate::i18n::t("Google OAuth 交换失败（HTTP {status}），请检查桌面应用的 Client ID、Secret 和 Drive API 配置"), &[("status", status.to_string())])));
    }
    let refresh = tokens["refresh_token"]
        .as_str()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| {
            error(crate::i18n::t(
                "Google 未签发离线刷新令牌，请重新授权并同意访问",
            ))
        })?;
    let access = text(&tokens, "access_token")?;
    Ok((refresh.into(), access.into()))
}

pub(super) fn validate(ctx: &sdk::RequestContext) -> Result<Value, RpcError> {
    let cfg = &ctx.params()["connection"];
    text(cfg, "clientId")?;
    text(cfg, "clientSecret")?;
    let existing = cfg["refreshToken"].as_str().is_some_and(|v| !v.is_empty());
    let (refresh, access) = if existing {
        match token(&ctx.http(), cfg) {
            Ok(access) => (None, access),
            Err(error)
                if error.data.as_ref().and_then(|data| data["reason"].as_str())
                    == Some("reauthorization_required") =>
            {
                let (refresh, access) = browser_authorization(ctx, cfg)?;
                (Some(refresh), access)
            }
            Err(error) => return Err(error),
        }
    } else {
        let (refresh, access) = browser_authorization(ctx, cfg)?;
        (Some(refresh), access)
    };
    http_request(
        &ctx.http(),
        &access,
        "GET",
        &format!("{API}/about?fields=user(displayName)"),
    )
    .send()
    .and_then(|r| r.error_for_status())
    .map_err(error)?;
    let result = sdk::ConnectionValidation::valid();
    Ok(match refresh {
        Some(refresh) => result.credential("refreshToken", refresh).into_value(),
        None => result.into_value(),
    })
}
