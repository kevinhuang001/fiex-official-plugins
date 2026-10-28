//! Connection provider plus mailbox methods.
use crate::{config::Settings, imap, smtp};
use fiex_plugin_sdk::{self as sdk, RpcError};
use serde_json::{json, Value};

fn text<'a>(p: &'a Value, key: &str) -> Result<&'a str, RpcError> {
    p.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && !value.contains('\0'))
        .ok_or_else(|| RpcError::invalid_params(format!("missing {key}")))
}

fn settings(ctx: &sdk::RequestContext, p: &Value) -> Result<Settings, RpcError> {
    Settings::from_params(ctx, p).map_err(RpcError::invalid_params)
}

fn imap_session(
    ctx: &sdk::RequestContext,
    settings: &Settings,
) -> Result<imap::Session<fiex_plugin_sdk::tcp::TcpStream>, String> {
    let stream = ctx
        .tcp()
        .connect(&settings.imap_host, 993)
        .map_err(|error| error.to_string())?;
    imap::login(
        stream,
        &settings.imap_host,
        &settings.email,
        &settings.password,
    )
}

pub fn run(ctx: sdk::RequestContext) -> Result<Value, RpcError> {
    let method = ctx.method();
    let p = ctx.params();
    match method {
        "email.test" => test_mailbox(&ctx, false),
        "email.list" => {
            let settings = settings(&ctx, p)?;
            let limit = p
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(12)
                .clamp(1, 30) as usize;
            let mut session = imap_session(&ctx, &settings).map_err(RpcError::internal)?;
            let messages = imap::list(&mut session, limit).map_err(RpcError::internal)?;
            let _ = session.logout();
            Ok(json!(messages))
        }
        "email.read" => {
            let settings = settings(&ctx, p)?;
            let seq =
                p.get("seq")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| RpcError::invalid_params("missing seq"))? as u32;
            let mut session = imap_session(&ctx, &settings).map_err(RpcError::internal)?;
            let message = imap::read(&mut session, seq).map_err(RpcError::internal)?;
            let _ = session.logout();
            Ok(json!(message))
        }
        "email.send" => {
            let settings = settings(&ctx, p)?;
            let to = text(p, "to")?.trim();
            if !crate::config::valid_address(to) {
                return Err(RpcError::invalid_params(crate::i18n::t("无效的收件人地址")));
            }
            let subject = p.get("subject").and_then(Value::as_str).unwrap_or("");
            let body = text(p, "body")?;
            let stream = ctx
                .tcp()
                .connect(&settings.smtp_host, 465)
                .map_err(|error| RpcError::internal(error.to_string()))?;
            smtp::send(
                stream,
                &settings.smtp_host,
                &settings.email,
                &settings.password,
                to,
                subject,
                body,
            )
            .map_err(RpcError::internal)?;
            Ok(json!({"ok": true}))
        }
        _ => Err(RpcError::method_not_found(method)),
    }
}

fn test_mailbox(ctx: &sdk::RequestContext, from_connection: bool) -> Result<Value, RpcError> {
    let p = ctx.params();

    let connection = p.get("connection").unwrap_or(p);
    let settings = if from_connection {
        Settings::from_connection(connection).map_err(RpcError::invalid_params)?
    } else {
        settings(ctx, p)?
    };
    let mut session = imap_session(ctx, &settings).map_err(RpcError::internal)?;
    let _ = session
        .select("INBOX")
        .map_err(|error| RpcError::internal(error.to_string()))?;
    drop(session);
    let stream = ctx
        .tcp()
        .connect(&settings.smtp_host, 465)
        .map_err(|error| RpcError::internal(error.to_string()))?;
    smtp::check(
        stream,
        &settings.smtp_host,
        &settings.email,
        &settings.password,
    )
    .map_err(RpcError::internal)?;
    Ok(json!({"imap": true, "smtp": true}))
}
pub struct EmailConnection;
impl sdk::ConnectionProvider for EmailConnection {
    fn validate(&self, ctx: &sdk::RequestContext) -> Result<Value, RpcError> {
        test_mailbox(ctx, true)
    }
    fn open(&self, ctx: &sdk::RequestContext) -> Result<Value, RpcError> {
        let p = ctx.params();

        let settings = Settings::from_connection(
            p.get("connection")
                .ok_or_else(|| RpcError::invalid_params("missing connection"))?,
        )
        .map_err(RpcError::invalid_params)?;
        let connection_id = text(p, "connectionId")?;
        Ok(json!({
            "kind": "pluginView",
            "view": {"source": "package", "entry": "ui/dist/index.html"},
            "tabTitle": fiex_plugin_sdk::i18n::format(crate::i18n::t("邮箱 · {0}"), &[("0", settings.email.clone())]),
            "state": {
                "connectionId": connection_id,
                "email": settings.email,
                "imapHost": settings.imap_host,
                "smtpHost": settings.smtp_host
            }
        }))
    }
}
