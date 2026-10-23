//! OpenPGP operations through the user's explicitly approved GnuPG executable.
use fiex_plugin_sdk::{self as sdk, HostServices, RpcError};
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
/// GnuPG reports machine-readable state on the status stream (`--status-fd=2`),
/// interleaved with human-readable text.
fn status_lines(stderr: &str) -> impl Iterator<Item = &str> {
    stderr
        .lines()
        .filter_map(|line| line.strip_prefix("[GNUPG:] "))
}
fn status_value<'a>(stderr: &'a str, keyword: &str) -> Option<&'a str> {
    status_lines(stderr).find_map(|line| {
        line.strip_prefix(keyword)
            .and_then(|rest| rest.trim().split(' ').next())
    })
}
fn signed_by(stderr: &str) -> Option<String> {
    status_lines(stderr).find_map(|line| {
        let rest = line.strip_prefix("GOODSIG ")?;
        Some(
            rest.split_once(' ')
                .map_or(String::new(), |(_, name)| name.to_string()),
        )
    })
}
/// A failed command still carries the useful diagnostic; human text is kept
/// short because it is rendered in the panel, not a log.
fn failure(stderr: &str) -> String {
    let message: Vec<&str> = stderr
        .lines()
        .filter(|line| !line.starts_with("[GNUPG:]") && !line.trim().is_empty())
        .collect();
    let text = if message.is_empty() {
        stderr.trim()
    } else {
        message.last().copied().unwrap_or("")
    };
    text.chars().take(240).collect()
}
fn keys(stdout: &str) -> Vec<Value> {
    let mut keys: Vec<Value> = Vec::new();
    let mut current: Option<(String, String, String, String)> = None;
    for line in stdout.lines() {
        let fields: Vec<&str> = line.split(':').collect();
        let record = fields.first().copied();
        match record {
            Some("pub") => {
                current = Some((
                    String::new(),
                    fields.get(4).copied().unwrap_or("").to_string(),
                    String::new(),
                    fields.get(6).copied().unwrap_or("").to_string(),
                ));
            }
            Some("fpr") if fields.len() > 9 => {
                if let Some(entry) = current.as_mut() {
                    if entry.0.is_empty() {
                        entry.0 = fields[9].to_string();
                    }
                }
            }
            Some("uid") if fields.len() > 9 => {
                if let Some(entry) = current.as_mut() {
                    if entry.2.is_empty() {
                        entry.2 = fields[9].to_string();
                    }
                }
            }
            _ => {}
        }
        // A key is complete once its primary fingerprint and uid are known.
        // Later subkeys must not overwrite the primary identity.
        if record != Some("pub") {
            if let Some((fingerprint, key_id, user, expires)) = current.as_ref() {
                if !fingerprint.is_empty() && !user.is_empty() {
                    keys.push(json!({
                        "fingerprint": fingerprint,
                        "keyId": key_id,
                        "name": user,
                        "email": user
                            .rsplit_once('<')
                            .and_then(|(_, rest)| rest.split_once('>'))
                            .map_or(String::new(), |(email, _)| email.to_string()),
                        "expires": expires,
                    }));
                    current = None;
                }
            }
        }
    }
    keys
}
fn run(method: &str, p: Value, host: &mut dyn HostServices) -> Result<Value, RpcError> {
    let result = host
        .process_run("gpg", &args(method, &p)?, None)
        .map_err(|error| {
            RpcError::internal(format!("无法运行 gpg，请先安装 GnuPG 并加入 PATH：{error}"))
        })?;
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
                let reason = if status_lines(&result.stderr).any(|line| line.starts_with("BADSIG"))
                {
                    "签名无效"
                } else if status_lines(&result.stderr)
                    .any(|line| line.starts_with("NO_PUBKEY") || line.starts_with("ERRSIG"))
                {
                    "缺少对应的公钥，或签名无法校验"
                } else {
                    "签名校验失败"
                };
                return Err(RpcError::internal(format!(
                    "{reason}：{}",
                    failure(&result.stderr)
                )));
            }
            return Ok(json!({
                "valid": true,
                "fingerprint": status_value(&result.stderr, "VALIDSIG").unwrap_or(""),
                "signer": signed_by(&result.stderr).unwrap_or_default(),
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
            failure(&result.stderr)
        )));
    }
    if method == "gpg.import" {
        let imported = status_lines(&result.stderr)
            .filter(|line| line.starts_with("IMPORT_OK"))
            .count();
        return Ok(json!({ "imported": imported, "status": failure(&result.stderr) }));
    }
    Ok(json!({ "ok": true, "status": result.stderr }))
}
fn main() {
    if let Err(error) = sdk::serve_with_host("org.fiex.gpg", run) {
        eprintln!("GPG plugin: {error}");
        std::process::exit(1);
    }
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
    fn parses_primary_keys_and_their_identity() {
        let list = "pub:u:4096:1:ABCDEF0123456789:1700000000:1800000000::u:::scESC:::::::\nfpr:::::::::0123456789ABCDEF0123456789ABCDEF01234567:\nuid:u::::1700000000::HASH::Ada Lovelace <ada@example.org>::::::::::0:\nsub:u:4096:1:1111222233334444:1700000000::::::e::::::\nfpr:::::::::9999888877776666555544443333222211110000:\n";
        let parsed = keys(list);
        assert_eq!(parsed.len(), 1);
        assert_eq!(
            parsed[0]["fingerprint"],
            "0123456789ABCDEF0123456789ABCDEF01234567"
        );
        assert_eq!(parsed[0]["keyId"], "ABCDEF0123456789");
        assert_eq!(parsed[0]["name"], "Ada Lovelace <ada@example.org>");
        assert_eq!(parsed[0]["email"], "ada@example.org");
    }
    #[test]
    fn verify_result_requires_a_good_signature() {
        let status = "[GNUPG:] NEWSIG\n[GNUPG:] GOODSIG ABCDEF0123456789 Ada Lovelace <ada@example.org>\n[GNUPG:] VALIDSIG 0123456789ABCDEF0123456789ABCDEF01234567 2024-01-01 1700000000 0 4 0 1 10 00 0123456789ABCDEF0123456789ABCDEF01234567\n";
        assert_eq!(
            status_value(status, "VALIDSIG"),
            Some("0123456789ABCDEF0123456789ABCDEF01234567")
        );
        assert_eq!(
            signed_by(status).as_deref(),
            Some("Ada Lovelace <ada@example.org>")
        );
    }
}
