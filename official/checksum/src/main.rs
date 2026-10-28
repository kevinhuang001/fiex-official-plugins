mod checksums;
#[path = "../../../support/i18n.rs"]
mod i18n;
#[path = "../../../support/resources.rs"]
mod resources;
use fiex_plugin_sdk::{self as sdk, RpcError};
use serde_json::{json, Value};

fn checksum_rows(
    ctx: &sdk::RequestContext,
    path: &sdk::ResourceRef,
) -> Result<Vec<Value>, RpcError> {
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
    match method {
        "checksum.calculate" => {
            let target = resources::input(&ctx, "path")?;
            let path = &target.reference;
            let files = ctx.files();
            let metadata = files
                .metadata(path)
                .map_err(|e| RpcError::internal(e.to_string()))?;
            let reader = files
                .open_read(path)
                .map_err(|e| RpcError::internal(e.to_string()))?;
            let hashes = checksums::calculate(reader).map_err(RpcError::internal)?;
            let mut result =
                json!({"ref": path, "name": resources::name(&target)?, "size": metadata.size});
            for hash in hashes {
                result[hash.algorithm.to_ascii_lowercase().replace('-', "")] = json!(hash.value);
            }
            Ok(result)
        }
        "checksum.info" => {
            let target = resources::input(&ctx, "path")?;
            let path = &target.reference;
            Ok(json!({"rows": checksum_rows(&ctx, path)?}))
        }
        _ => Err(RpcError::method_not_found(method)),
    }
}
fn main() {
    if let Err(e) = sdk::Providers::new()
        .info(Presentation)
        .serve_with("org.fiex.checksum", run)
    {
        eprintln!("Checksum plugin failed: {e}");
        std::process::exit(1);
    }
}

#[path = "../../../support/provider_results.rs"]
mod provider_results;
pub(crate) struct Presentation;
impl sdk::InfoProvider for Presentation {
    fn resolve(&self, ctx: &sdk::RequestContext) -> Result<Vec<sdk::InfoSection>, RpcError> {
        let target = ctx
            .target()
            .ok_or_else(|| RpcError::invalid_params("missing target"))?;
        provider_results::section(
            "checksum",
            crate::i18n::t("校验和"),
            serde_json::to_value(checksum_rows(ctx, &target.reference)?)
                .map_err(|error| RpcError::internal(error.to_string()))?,
        )
    }
}
