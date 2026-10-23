mod archives;
mod ignores;
use fiex_plugin_sdk::{self as sdk, RpcError};
use serde_json::{json, Value};
fn param<'a>(p: &'a Value, k: &str) -> Result<&'a str, RpcError> {
    p.get(k)
        .and_then(Value::as_str)
        .ok_or_else(|| RpcError::invalid_params(format!("missing {k}")))
}
/// Settings are comma or newline separated; blank entries and comments are
/// ignored by `IgnoreSet` as well.
fn stored_ignores(host: &mut dyn sdk::HostServices, key: &str) -> Vec<String> {
    match host.storage_get("global", key) {
        Ok(Value::String(value)) => value
            .split(['\n', ',', '，'])
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}
fn merge_ignores(params: &Value, mut stored: Vec<String>) -> Vec<String> {
    let mut values = params
        .get("ignores")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    values.append(&mut stored);
    values
}
fn run(method: &str, p: Value, host: &mut dyn sdk::HostServices) -> Result<Value, RpcError> {
    match method {
        "archive.compress" => {
            let paths = p
                .get("paths")
                .and_then(Value::as_array)
                .ok_or_else(|| RpcError::invalid_params("missing paths"))?
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(str::to_string)
                        .ok_or_else(|| RpcError::invalid_params("invalid path"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let use_settings = p
                .get("ignoreSettings")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let stored = if use_settings {
                stored_ignores(host, "compressIgnores")
            } else {
                Vec::new()
            };
            let ignores = merge_ignores(&p, stored);
            let format = param(&p, "format")?;
            let destination = match p.get("destination").and_then(Value::as_str) {
                Some(value) => value.to_owned(),
                None => archives::default_destination(&paths, format, host)
                    .map_err(RpcError::internal)?,
            };
            let result = archives::compress(
                &paths,
                &destination,
                format,
                &ignores,
                p.get("level").and_then(Value::as_u64).map(|n| n as u32),
                p.get("password").and_then(Value::as_str),
                host,
            )
            .map_err(RpcError::internal)?;
            Ok(json!(result))
        }
        "archive.extract" => {
            let stored = stored_ignores(host, "extractIgnores");
            let ignores = merge_ignores(&p, stored);
            Ok(json!(archives::extract(
                param(&p, "path")?,
                p.get("destination").and_then(Value::as_str),
                p.get("password").and_then(Value::as_str),
                &ignores,
                host
            )
            .map_err(RpcError::internal)?))
        }
        _ => Err(RpcError::method_not_found(method)),
    }
}
fn main() {
    if let Err(e) = sdk::serve_with_host("org.fiex.archive", |method, p, host| {
        let method = method.to_owned();
        sdk::worker::with_host_worker(host, move |mut proxy| run(&method, p, &mut proxy))
    }) {
        eprintln!("Archive plugin failed: {e}");
        std::process::exit(1);
    }
}
