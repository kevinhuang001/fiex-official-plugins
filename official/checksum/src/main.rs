mod checksums;
use fiex_plugin_sdk::{self as sdk, RpcError};
use serde_json::{json, Value};
fn run(method: &str, p: Value, host: &mut dyn sdk::HostServices) -> Result<Value, RpcError> {
    match method {
        "checksum.calculate" => {
            let path = p
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| RpcError::invalid_params("missing path"))?;
            let files = host.files();
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
        _ => Err(RpcError::method_not_found(method)),
    }
}
fn main() {
    if let Err(e) = sdk::serve_with_host("org.fiex.checksum", run) {
        eprintln!("Checksum plugin failed: {e}");
        std::process::exit(1);
    }
}
