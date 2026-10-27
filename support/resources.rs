//! Helpers for the official providers' ref-only request parameters.
use fiex_plugin_sdk::{RequestContext, ResourceContext, ResourceRef, RpcError};

pub fn input(ctx: &RequestContext, key: &str) -> Result<ResourceContext, RpcError> {
    let value = &ctx.params()[key];
    let reference = if value.is_object() {
        serde_json::from_value::<ResourceRef>(value.get("ref").unwrap_or(value).clone())
            .map_err(|error| RpcError::invalid_params(error.to_string()))?
    } else {
        ctx.target()
            .ok_or_else(|| RpcError::invalid_params("missing resource ref"))?
            .reference
            .clone()
    };
    let metadata = ctx
        .targets()
        .iter()
        .find(|target| target.reference == reference)
        .and_then(|target| target.metadata.clone())
        .or_else(|| {
            ctx.directory()
                .filter(|target| target.reference == reference)
                .and_then(|target| target.metadata)
        })
        .or_else(|| ctx.files().metadata(&reference).ok());
    Ok(ResourceContext {
        reference,
        metadata,
    })
}
pub fn name(target: &ResourceContext) -> Result<&str, RpcError> {
    target
        .name()
        .ok_or_else(|| RpcError::invalid_params("target metadata is required"))
}
