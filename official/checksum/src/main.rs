use fiex_plugin_sdk::ProviderInvocationExt;
mod checksums;
#[path = "../../../support/provider_results.rs"]
mod provider_results;
#[path = "../../../support/resources.rs"]
mod resources;
use fiex_plugin_sdk::{self as sdk, InfoFieldKey, RpcError};
use serde_json::{json, Value};

fn run(ctx: sdk::RequestContext) -> Result<Value, RpcError> {
    let method = ctx.method();
    match method {
        "checksum.calculate" => {
            let target = resources::request_input(&ctx, "path")?;
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
        _ => Err(RpcError::method_not_found(method)),
    }
}
fn main() {
    if let Err(e) = sdk::Providers::new()
        .diagnostics()
        .info(Presentation)
        .serve_with("org.fiex.checksum", run)
    {
        eprintln!("Checksum plugin failed: transport closed ({:?})", e.kind());
        std::process::exit(1);
    }
}

pub(crate) struct Presentation;
impl sdk::InfoProvider for Presentation {
    fn compute(
        &self,
        ctx: &dyn sdk::ProviderInvocation,
        fields: &[InfoFieldKey],
    ) -> Result<Vec<sdk::InfoFieldResult>, RpcError> {
        let requested = fields
            .iter()
            .map(|field| {
                if field.subsection_id != "checksums" {
                    return Err(RpcError::invalid_params("unknown checksum section"));
                }
                let algorithm = match field.field_id.as_str() {
                    "sha256" => "sha256",
                    "sha512" => "sha512",
                    "sha1" => "sha1",
                    "md5" => "md5",
                    "crc32" => "crc32",
                    _ => return Err(RpcError::invalid_params("unknown checksum field")),
                };
                Ok(algorithm)
            })
            .collect::<Result<Vec<_>, _>>()?;
        if requested.is_empty() {
            return Ok(Vec::new());
        }
        let target = ctx
            .target()
            .ok_or_else(|| RpcError::invalid_params("missing target"))?;
        let reader = ctx
            .files()
            .open_read(&target.reference)
            .map_err(|error| RpcError::internal(error.to_string()))?;
        let values = checksums::calculate_selected(reader, &requested)
            .map_err(RpcError::internal)?
            .into_iter()
            .map(|hash| {
                let id = match hash.algorithm {
                    "SHA-256" => "sha256",
                    "SHA-512" => "sha512",
                    "SHA-1" => "sha1",
                    "MD5" => "md5",
                    "CRC32" => "crc32",
                    _ => unreachable!("calculate_selected returns known algorithms"),
                };
                (id, hash.value)
            })
            .collect::<std::collections::HashMap<_, _>>();
        fields
            .iter()
            .map(|field| {
                values
                    .get(field.field_id.as_str())
                    .map(|value| {
                        provider_results::result(&field.subsection_id, &field.field_id, value)
                    })
                    .ok_or_else(|| RpcError::internal("checksum result missing"))
            })
            .collect()
    }
}
