mod checksums;
use fiex_plugin_sdk::{self as sdk, RpcError};
use serde_json::{json, Value};
fn run(ctx: sdk::RequestContext) -> Result<Value, RpcError> {
    let method = ctx.method();
    let p = ctx.params();
    match method {
        "checksum.calculate" => {
            let path = p
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| RpcError::invalid_params("missing path"))?;
            let files = ctx.files();
            let metadata = files
                .metadata(path)
                .map_err(|e| RpcError::internal(e.to_string()))?;
            let reader = files
                .open_read(path)
                .map_err(|e| RpcError::internal(e.to_string()))?;
            let hashes = checksums::calculate(reader).map_err(RpcError::internal)?;
            let mut result = json!({"path": path, "size": metadata.size});
            for hash in hashes {
                result[hash.algorithm.to_ascii_lowercase().replace('-', "")] = json!(hash.value);
            }
            Ok(result)
        }
        "checksum.info" => {
            let path = ctx
                .params()
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| RpcError::invalid_params("missing path"))?;
            let files = ctx.files();
            let metadata = files
                .metadata(path)
                .map_err(|error| RpcError::internal(error.to_string()))?;
            let reader = files
                .open_read(path)
                .map_err(|error| RpcError::internal(error.to_string()))?;
            let hashes = checksums::calculate(reader).map_err(RpcError::internal)?;
            let mut rows =
                vec![serde_json::json!({"label":"大小","value":metadata.size.to_string()})];
            for hash in hashes {
                rows.push(serde_json::json!({
                    "label": hash.algorithm.to_ascii_uppercase(),
                    "value": hash.value,
                }));
            }
            Ok(serde_json::json!({"rows": rows}))
        }
        _ => Err(RpcError::method_not_found(method)),
    }
}
fn main() {
    if let Err(e) = sdk::serve("org.fiex.checksum", run) {
        eprintln!("Checksum plugin failed: {e}");
        std::process::exit(1);
    }
}
