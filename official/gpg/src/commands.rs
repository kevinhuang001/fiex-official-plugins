//! Method dispatch: turn a panel request into a GnuPG invocation and shape the
//! result for the panel.
use crate::{gpg, keys::keyring, keys::keys};
use fiex_plugin_sdk::{self as sdk, RpcError};
use serde_json::{json, Value};

fn text<'a>(p: &'a Value, key: &str) -> Result<&'a str, RpcError> {
    p.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && !s.contains('\0'))
        .ok_or_else(|| RpcError::invalid_params(format!("missing {key}")))
}
fn flag(p: &Value, key: &str) -> bool {
    p.get(key).and_then(Value::as_bool).unwrap_or(false)
}
fn fingerprint(p: &Value) -> Result<&str, RpcError> {
    let value = text(p, "fingerprint")?;
    if !matches!(value.len(), 40 | 64) || !value.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(RpcError::invalid_params("select a full key fingerprint"));
    }
    Ok(value)
}
fn key_identity(p: &Value) -> Result<String, RpcError> {
    let name = text(p, "name")?.trim();
    let email = text(p, "email")?.trim();
    if name.is_empty()
        || name.len() > 64
        || name.starts_with('-')
        || name
            .chars()
            .any(|c| c.is_control() || matches!(c, '<' | '>' | '\0'))
        || email.is_empty()
        || email.len() > 254
        || !email.contains('@')
        || email
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || matches!(c, '<' | '>' | '\0'))
    {
        return Err(RpcError::invalid_params("invalid key name or email"));
    }
    Ok(format!("{name} <{email}>"))
}

fn key_expiry(p: &Value) -> Result<String, RpcError> {
    let expiry = p.get("expiry").and_then(Value::as_str).unwrap_or("0");
    if !matches!(expiry, "0" | "1y" | "2y" | "5y") {
        return Err(RpcError::invalid_params("expiry must be 0, 1y, 2y or 5y"));
    }
    Ok(expiry.to_string())
}

fn key_passphrase(p: &Value) -> Result<String, RpcError> {
    let passphrase = p.get("passphrase").and_then(Value::as_str).unwrap_or("");
    if passphrase.len() > 1024 || passphrase.contains(['\0', '\n', '\r']) {
        return Err(RpcError::invalid_params("invalid key passphrase"));
    }
    Ok(passphrase.to_string())
}

fn args(method: &str, p: &Value) -> Result<Vec<String>, RpcError> {
    let mut args = vec!["--batch".into(), "--no-tty".into(), "--status-fd=2".into()];
    match method {
        "gpg.info" => args.push("--version".into()),
        "gpg.keys" => {
            args.extend(["--with-colons", "--fingerprint", "--list-keys"].map(String::from))
        }
        "gpg.secretKeys" => {
            args.extend(["--with-colons", "--fingerprint", "--list-secret-keys"].map(String::from))
        }
        "gpg.createKey" => {
            let identity = key_identity(p)?;
            let expiry = key_expiry(p)?;
            args.extend(
                [
                    "--pinentry-mode",
                    "loopback",
                    "--passphrase-fd",
                    "0",
                    "--quick-generate-key",
                    &identity,
                    "default",
                    "default",
                    &expiry,
                ]
                .map(String::from),
            );
        }
        "gpg.import" => args.extend(["--import", "--", text(p, "input")?].map(String::from)),
        "gpg.deleteKey" => {
            let command = if flag(p, "secret") {
                "--delete-secret-and-public-keys"
            } else {
                "--delete-keys"
            };
            args.extend(["--yes", command, "--", fingerprint(p)?].map(String::from));
        }
        "gpg.verify" => args
            .extend(["--verify", "--", text(p, "signature")?, text(p, "input")?].map(String::from)),
        "gpg.encrypt" | "gpg.decrypt" | "gpg.sign" | "gpg.export" => {
            let output = text(p, "output")?;
            if p.get("input").and_then(Value::as_str) == Some(output) {
                return Err(RpcError::invalid_params("input and output must differ"));
            }
            args.extend(["--output", output].map(String::from));
            // Armor describes produced text; decrypting produces the original
            // payload, so the flag would only confuse the operand order.
            if method != "gpg.decrypt" && flag(p, "armor") {
                args.push("--armor".into());
            }
            match method {
                "gpg.encrypt" => {
                    // Whether an unverified key is acceptable is the user's
                    // decision in the panel: GnuPG's web-of-trust prompt cannot
                    // be answered from a non-interactive command.
                    if flag(p, "trustUnverified") {
                        args.extend(["--trust-model", "always"].map(String::from));
                    }
                    args.extend(
                        [
                            "--recipient",
                            fingerprint(p)?,
                            "--encrypt",
                            "--",
                            text(p, "input")?,
                        ]
                        .map(String::from),
                    )
                }
                "gpg.decrypt" => {
                    args.extend(["--decrypt", "--", text(p, "input")?].map(String::from))
                }
                "gpg.sign" => args.extend(
                    [
                        "--local-user",
                        fingerprint(p)?,
                        "--detach-sign",
                        "--",
                        text(p, "input")?,
                    ]
                    .map(String::from),
                ),
                _ => {
                    let command = if flag(p, "secret") {
                        "--export-secret-keys"
                    } else {
                        "--export"
                    };
                    args.extend([command, "--", fingerprint(p)?].map(String::from));
                }
            }
        }
        _ => return Err(RpcError::method_not_found(method)),
    }
    Ok(args)
}

pub fn run(ctx: sdk::RequestContext) -> Result<Value, RpcError> {
    let method = ctx.method();
    let p = ctx.params();
    if method == "contextMenu.resolve" {
        let count = p["selectionCount"].as_u64().unwrap_or(1);
        if count == 0 {
            return Ok(json!({"items":[]}));
        }
        let mut items = vec![json!({"id":"gpg.menu","title":"GPG","kind":"submenu"})];
        for (id, title) in [
            ("gpg.encrypt", "加密…"),
            ("gpg.decrypt", "解密…"),
            ("gpg.sign", "签名…"),
        ] {
            items.push(json!({"id":id,"title":title,"kind":"action","parentId":"gpg.menu"}));
        }
        if count == 1 {
            items.push(json!({"id":"gpg.verify","title":"验证签名…","kind":"action","parentId":"gpg.menu"}));
        }
        return Ok(json!({"items":items}));
    }
    if method == "contextMenu.execute" {
        let item = p["itemId"]
            .as_str()
            .ok_or_else(|| RpcError::invalid_params("missing menu itemId"))?;
        let (title, width, height) = match item {
            "gpg.encrypt" => ("GPG 加密", 560, 520),
            "gpg.decrypt" => ("GPG 解密", 520, 400),
            "gpg.sign" => ("GPG 签名", 540, 470),
            "gpg.verify" if p["paths"].as_array().is_some_and(|paths| paths.len() == 1) => {
                ("验证签名", 520, 420)
            }
            _ => {
                return Err(RpcError::invalid_params(
                    "unknown or invalid GPG menu action",
                ))
            }
        };
        return Ok(json!({
            "kind":"panel", "entry":"ui/dist/index.html",
            "panelId":item, "title":title,
            "size":{"width":width,"height":height}
        }));
    }
    if method == "gpg.keys" {
        let public = gpg::invoke(&args("gpg.keys", p)?, &ctx)?;
        if public.code != Some(0) {
            return Err(RpcError::internal(format!(
                "GnuPG：{}",
                gpg::failure(&public.stderr)
            )));
        }
        // A keyring without secret keys is normal; treat that listing as empty
        // instead of failing public key management.
        let secret = gpg::invoke(&args("gpg.secretKeys", p)?, &ctx)
            .map(|result| result.stdout)
            .unwrap_or_default();
        return Ok(json!(keyring(&public.stdout, &secret)));
    }
    if method == "gpg.createKey" {
        let stdin = format!("{}\n", key_passphrase(p)?);
        let result = gpg::invoke_with_stdin(&args(method, p)?, &ctx, Some(&stdin))?;
        if result.code != Some(0) {
            return Err(RpcError::internal(format!(
                "GnuPG：{}",
                gpg::failure(&result.stderr)
            )));
        }
        let fingerprint = gpg::status_line(&result.stderr, "KEY_CREATED")
            .and_then(|rest| rest.split_whitespace().last())
            .unwrap_or("")
            .to_string();
        return Ok(json!({
            "ok": true,
            "fingerprint": fingerprint,
            "status": result.stderr
        }));
    }
    let result = gpg::invoke(&args(method, p)?, &ctx)?;
    match method {
        "gpg.info" => {
            let version = result
                .stdout
                .lines()
                .next()
                .unwrap_or("GnuPG")
                .trim()
                .to_string();
            return Ok(json!({ "version": version }));
        }
        "gpg.keys" => return Ok(json!(keys(&result.stdout))),
        "gpg.verify" => {
            if result.code != Some(0) {
                let reason =
                    if gpg::status_lines(&result.stderr).any(|line| line.starts_with("BADSIG")) {
                        "签名无效"
                    } else if gpg::status_lines(&result.stderr)
                        .any(|line| line.starts_with("NO_PUBKEY") || line.starts_with("ERRSIG"))
                    {
                        "缺少对应的公钥，或签名无法校验"
                    } else {
                        "签名校验失败"
                    };
                return Err(RpcError::internal(format!(
                    "{reason}：{}",
                    gpg::failure(&result.stderr)
                )));
            }
            return Ok(json!({
                "valid": true,
                "fingerprint": gpg::status_value(&result.stderr, "VALIDSIG").unwrap_or(""),
                "signer": gpg::signed_by(&result.stderr).unwrap_or_default(),
            }));
        }
        _ => {}
    }
    if result.truncated {
        return Err(RpcError::internal("GnuPG output exceeded the limit"));
    }
    if result.code != Some(0) {
        return Err(RpcError::internal(format!(
            "GnuPG：{}",
            gpg::failure(&result.stderr)
        )));
    }
    if method == "gpg.import" {
        let imported = gpg::status_lines(&result.stderr)
            .filter(|line| line.starts_with("IMPORT_OK"))
            .count();
        return Ok(json!({ "imported": imported, "status": gpg::failure(&result.stderr) }));
    }
    Ok(json!({ "ok": true, "status": result.stderr }))
}

#[cfg(test)]
mod tests;
