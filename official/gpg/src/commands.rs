//! Method dispatch: turn a panel request into a GnuPG invocation and shape the
//! result for the panel.
use crate::{gpg, keys::keys};
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
fn args(method: &str, p: &Value) -> Result<Vec<String>, RpcError> {
    let mut args = vec!["--batch".into(), "--no-tty".into(), "--status-fd=2".into()];
    match method {
        "gpg.info" => args.push("--version".into()),
        "gpg.keys" => {
            args.extend(["--with-colons", "--fingerprint", "--list-keys"].map(String::from))
        }
        "gpg.import" => args.extend(["--import", "--", text(p, "input")?].map(String::from)),
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
                _ => args.extend(["--export", "--", fingerprint(p)?].map(String::from)),
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
        let count = p["selectionCount"].as_u64().unwrap_or(0);
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
mod tests {
    use super::*;
    #[test]
    fn filenames_are_operands_and_output_is_not_overwritten() {
        let a = args(
            "gpg.decrypt",
            &json!({"input":"--options","output":"plain"}),
        )
        .unwrap();
        assert_eq!(&a[a.len() - 2..], &["--", "--options"]);
        assert!(!a.contains(&"--yes".to_string()));
        assert!(args("gpg.decrypt", &json!({"input":"same","output":"same"})).is_err());
    }
    #[test]
    fn rejects_arbitrary_commands_and_key_options() {
        assert!(args("gpg.delete", &json!({})).is_err());
        assert!(args(
            "gpg.encrypt",
            &json!({"input":"x","output":"y","fingerprint":"--foo"})
        )
        .is_err());
    }
    #[test]
    fn armor_and_trust_are_explicit_flags() {
        let plain = args(
            "gpg.encrypt",
            &json!({"input":"a","output":"b","fingerprint":"A".repeat(40)}),
        )
        .unwrap();
        assert!(!plain.contains(&"--armor".to_string()));
        assert!(!plain.contains(&"--trust-model".to_string()));
        let full = args(
            "gpg.encrypt",
            &json!({"input":"a","output":"b","fingerprint":"A".repeat(40),"armor":true,"trustUnverified":true}),
        )
        .unwrap();
        assert!(full.contains(&"--armor".to_string()));
        assert!(full.windows(2).any(|w| w == ["--trust-model", "always"]));
    }
    #[test]
    fn encrypt_arguments_are_complete_and_ordered() {
        let fpr = "A".repeat(40);
        let args = args(
            "gpg.encrypt",
            &json!({"input":"in.txt","output":"out.gpg","fingerprint":fpr}),
        )
        .unwrap();
        assert!(args.contains(&"--batch".to_string()));
        assert!(args.contains(&"--no-tty".to_string()));
        assert!(args.contains(&"--status-fd=2".to_string()));
        assert!(args.windows(2).any(|w| w == ["--output", "out.gpg"]));
        assert!(args.windows(2).any(|w| w == ["--recipient", fpr.as_str()]));
        let encrypt = args.iter().position(|a| a == "--encrypt").unwrap();
        let separator = args.iter().position(|a| a == "--").unwrap();
        assert!(
            encrypt < separator,
            "options must precede the operand separator"
        );
        assert_eq!(&args[separator + 1..], &["in.txt"]);
    }

    #[test]
    fn decrypt_never_requests_armor_even_if_asked() {
        let args = args(
            "gpg.decrypt",
            &json!({"input":"in.gpg","output":"out","armor":true}),
        )
        .unwrap();
        assert!(!args.contains(&"--armor".to_string()));
        assert!(args.windows(2).any(|w| w == ["--output", "out"]));
        assert_eq!(&args[args.len() - 2..], &["--", "in.gpg"]);
    }

    #[test]
    fn signing_uses_the_selected_identity_and_optional_armor() {
        let fpr = "B".repeat(40);
        let plain = args(
            "gpg.sign",
            &json!({"input":"a","output":"a.sig","fingerprint":fpr}),
        )
        .unwrap();
        assert!(plain
            .windows(2)
            .any(|w| w == ["--local-user", fpr.as_str()]));
        assert!(plain.contains(&"--detach-sign".to_string()));
        assert!(!plain.contains(&"--armor".to_string()));
        let armored = args(
            "gpg.sign",
            &json!({"input":"a","output":"a.sig.asc","fingerprint":fpr,"armor":true}),
        )
        .unwrap();
        assert!(armored.contains(&"--armor".to_string()));
    }

    #[test]
    fn export_and_import_use_the_operand_separator() {
        let fpr = "C".repeat(40);
        let export = args(
            "gpg.export",
            &json!({"output":"pub.asc","fingerprint":fpr,"armor":true}),
        )
        .unwrap();
        let separator = export.iter().position(|a| a == "--").unwrap();
        assert_eq!(&export[separator + 1..], &[fpr.as_str()]);
        let import = args("gpg.import", &json!({"input":"pub.asc"})).unwrap();
        assert_eq!(&import[import.len() - 2..], &["--", "pub.asc"]);
        let verify = args("gpg.verify", &json!({"signature":"a.sig","input":"a"})).unwrap();
        assert_eq!(&verify[verify.len() - 3..], &["--", "a.sig", "a"]);
        let info = args("gpg.info", &json!({})).unwrap();
        assert!(info.contains(&"--version".to_string()));
    }

    #[test]
    fn fingerprints_must_be_full_and_hexadecimal() {
        let not_hex = "Z".repeat(40);
        for bad in ["", "ABCDEF", "--recipient", not_hex.as_str()] {
            assert!(
                args(
                    "gpg.encrypt",
                    &json!({"input":"a","output":"b","fingerprint":bad})
                )
                .is_err(),
                "fingerprint must be rejected: {bad:?}"
            );
        }
        let hex = "0123456789abcdef".repeat(4);
        for len in [40usize, 64] {
            let fpr = &hex[..len];
            assert!(args(
                "gpg.encrypt",
                &json!({"input":"a","output":"b","fingerprint":fpr})
            )
            .is_ok());
        }
    }
}
