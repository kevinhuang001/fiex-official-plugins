mod checksums;
use fiex_plugin_sdk::{self as sdk, RpcError};
use serde_json::{json, Value};
fn checksum_rows(ctx: &sdk::RequestContext, path: &str) -> Result<Vec<Value>, RpcError> {
    let files = ctx.files();
    let reader = files
        .open_read(path)
        .map_err(|error| RpcError::internal(error.to_string()))?;
    hash_rows(reader)
}

fn hash_rows(reader: impl std::io::Read) -> Result<Vec<Value>, RpcError> {
    let hashes = checksums::calculate(reader).map_err(RpcError::internal)?;
    Ok(hashes
        .into_iter()
        .map(|hash| {
            serde_json::json!({
                "label": hash.algorithm.to_ascii_uppercase(),
                "value": hash.value,
                "copyable": true,
            })
        })
        .collect())
}

fn run(ctx: sdk::RequestContext) -> Result<Value, RpcError> {
    let method = ctx.method();
    let p = ctx.params();
    if method == "info.resolve" {
        let reference = ctx
            .target()
            .and_then(|target| target.get("ref"))
            .ok_or_else(|| RpcError::invalid_params("missing target"))?;
        let reference: sdk::ResourceRef = serde_json::from_value(reference.clone())
            .map_err(|error| RpcError::invalid_params(error.to_string()))?;
        let reader = ctx
            .files()
            .open_ref(&reference)
            .map_err(|error| RpcError::internal(error.to_string()))?;
        let rows = hash_rows(reader)?;
        return Ok(json!({"sections":[{"id":"checksum","title":"校验和","rows":rows}]}));
    }
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
            Ok(json!({"rows": checksum_rows(&ctx, path)?}))
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
