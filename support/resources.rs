//! Helpers for typed provider resources and custom RPC resource refs.
use fiex_plugin_sdk::ProviderInvocationExt;
use fiex_plugin_sdk::{RequestContext, ResourceContext, ResourceRef, RpcError};
use serde_json::Value;

pub fn request_input(ctx: &RequestContext, key: &str) -> Result<ResourceContext, RpcError> {
    from_params(ctx.params(), key, ctx)
}

pub fn from_params(
    params: &Value,
    key: &str,
    ctx: &dyn fiex_plugin_sdk::ProviderInvocation,
) -> Result<ResourceContext, RpcError> {
    let value = &params[key];
    if !value.is_null() && !value.is_object() {
        return Err(RpcError::invalid_params(
            "resource arguments must be ResourceRefs",
        ));
    }
    let reference = if value.is_object() {
        serde_json::from_value::<ResourceRef>(value.get("ref").unwrap_or(value).clone())
            .map_err(|error| RpcError::invalid_params(error.to_string()))?
    } else {
        ctx.target()
            .ok_or_else(|| RpcError::invalid_params("missing resource ref"))?
            .reference
            .clone()
    };
    let authorized = ctx
        .targets()
        .iter()
        .find(|target| target.reference == reference)
        .or_else(|| {
            ctx.directory()
                .filter(|target| target.reference == reference)
        });
    let metadata = authorized
        .and_then(|target| target.metadata.clone())
        .or_else(|| ctx.files().metadata(&reference).ok());
    Ok(ResourceContext {
        reference,
        metadata,
        view: authorized.and_then(|target| target.view.clone()),
        storage: authorized.and_then(|target| target.storage.clone()),
        capabilities: authorized
            .map(|target| target.capabilities.clone())
            .unwrap_or_default(),
    })
}

pub fn target(ctx: &dyn fiex_plugin_sdk::ProviderInvocation) -> Result<ResourceContext, RpcError> {
    let target = ctx
        .target()
        .ok_or_else(|| RpcError::invalid_params("missing resource ref"))?;
    let mut target = target.clone();
    if target.metadata.is_none() {
        target.metadata = ctx.files().metadata(&target.reference).ok();
    }
    Ok(target)
}
pub fn name(target: &ResourceContext) -> Result<&str, RpcError> {
    target
        .name()
        .ok_or_else(|| RpcError::invalid_params("target metadata is required"))
}
