//! Account settings for a connection or for the standalone toolbar panel.
use fiex_plugin_sdk as sdk;
use serde_json::Value;

#[derive(Clone, Debug)]
pub struct Settings {
    pub connection_id: Option<String>,
    pub email: String,
    pub password: String,
    pub imap_host: String,
    pub smtp_host: String,
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
}

pub fn valid_address(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 254
        && value.contains('@')
        && !value
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || matches!(c, '<' | '>' | '\0'))
}

pub fn valid_host(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 253
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'))
}

impl Settings {
    pub fn from_connection(value: &Value) -> Result<Self, String> {
        Self::from_fields(None, value)
    }

    pub fn from_params(ctx: &sdk::RequestContext, params: &Value) -> Result<Self, String> {
        let id = params
            .get("connectionId")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .ok_or(crate::i18n::t("请从连接中打开邮箱"))?
            .to_string();
        let mut fields = params.clone();
        let provided = fields
            .get("password")
            .and_then(Value::as_str)
            .is_some_and(|password| !password.is_empty());
        if !provided {
            let password = ctx
                .secrets()
                .scoped(&format!("connection-{id}"))
                .get::<String>("password")
                .map_err(|error| error.to_string())?
                .ok_or(crate::i18n::t("邮箱密码未设置"))?;
            if let Some(object) = fields.as_object_mut() {
                object.insert("password".into(), Value::String(password));
            }
        }
        Self::from_fields(Some(id), &fields)
    }

    fn from_fields(connection_id: Option<String>, value: &Value) -> Result<Self, String> {
        let email = text(value, "email");
        let password = text(value, "password");
        let imap_host = {
            let host = text(value, "imapHost");
            if host.is_empty() {
                "imap.qq.com".to_string()
            } else {
                host
            }
        };
        let smtp_host = {
            let host = text(value, "smtpHost");
            if host.is_empty() {
                "smtp.qq.com".to_string()
            } else {
                host
            }
        };
        Self::validate(connection_id, email, password, imap_host, smtp_host)
    }

    fn validate(
        connection_id: Option<String>,
        email: String,
        password: String,
        imap_host: String,
        smtp_host: String,
    ) -> Result<Self, String> {
        if !valid_address(&email) {
            return Err(crate::i18n::t("请填写有效的邮箱地址").into());
        }
        if password.is_empty() {
            return Err(crate::i18n::t("请填写 IMAP/SMTP 授权码或密码").into());
        }
        if !valid_host(&imap_host) || !valid_host(&smtp_host) {
            return Err(crate::i18n::t("IMAP/SMTP 服务器地址无效").into());
        }
        Ok(Self {
            connection_id,
            email,
            password,
            imap_host,
            smtp_host,
        })
    }
}
