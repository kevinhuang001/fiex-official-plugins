//! OpenPGP operations through the user's explicitly approved GnuPG executable.
use fiex_plugin_sdk::{self as sdk, HostServices, RpcError};
use serde_json::{json, Value};
fn text<'a>(p: &'a Value, key: &str) -> Result<&'a str, RpcError> {
    p.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && !s.contains('\0'))
        .ok_or_else(|| RpcError::invalid_params(format!("missing {key}")))
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
            match method {
                "gpg.encrypt" => args.extend(
                    [
                        "--recipient",
                        fingerprint(p)?,
                        "--encrypt",
                        "--",
                        text(p, "input")?,
                    ]
                    .map(String::from),
                ),
                "gpg.decrypt" => {
                    args.extend(["--decrypt", "--", text(p, "input")?].map(String::from))
                }
                "gpg.sign" => args.extend(
                    [
                        "--local-user",
                        fingerprint(p)?,
                        "--armor",
                        "--detach-sign",
                        "--",
                        text(p, "input")?,
                    ]
                    .map(String::from),
                ),
                _ => args.extend(["--armor", "--export", "--", fingerprint(p)?].map(String::from)),
            }
        }
        _ => return Err(RpcError::method_not_found(method)),
    }
    Ok(args)
}
fn run(method: &str, p: Value, host: &mut dyn HostServices) -> Result<Value, RpcError> {
    let result = host.process_run("gpg", &args(method, &p)?, None)?;
    if result.truncated {
        return Err(RpcError::internal("GnuPG output exceeded the limit"));
    }
    if result.code != Some(0) {
        return Err(RpcError::internal(format!("GnuPG: {}", result.stderr)));
    }
    if method == "gpg.keys" {
        let mut keys = Vec::new();
        let mut current = String::new();
        for line in result.stdout.lines() {
            let fields: Vec<_> = line.split(':').collect();
            if fields.first() == Some(&"pub") {
                current.clear();
            }
            if fields.first() == Some(&"fpr") && current.is_empty() && fields.len() > 9 {
                current = fields[9].into();
            }
            if fields.first() == Some(&"uid") && fields.len() > 9 && !current.is_empty() {
                keys.push(json!({"fingerprint":current,"name":fields[9]}));
            }
        }
        return Ok(json!(keys));
    }
    Ok(json!({"ok":true,"status":result.stderr}))
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
}
