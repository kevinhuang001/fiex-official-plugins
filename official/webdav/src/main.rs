//! Standalone WebDAV adapter. The Fiex host owns mount configuration,
//! credential storage, cache, sync rules and conflict resolution.
mod client;
mod discovery;
mod fs;
#[path = "../../../support/i18n.rs"]
mod i18n;

use fiex_plugin_sdk::{
    self as sdk,
    filesystem::{VfsProvider, FsEntry, FsListing, FsRead, FsRequest, FsWrite},
};

struct WebDav;

impl VfsProvider for WebDav {
    fn discover(
        &self,
        ctx: &sdk::RequestContext,
    ) -> Result<Vec<sdk::filesystem::FsOperation>, sdk::RpcError> {
        discovery::discover(ctx)
    }

    fn operations(&self) -> &'static [sdk::filesystem::FsOperation] {
        &[
            sdk::filesystem::FsOperation::List,
            sdk::filesystem::FsOperation::Stat,
            sdk::filesystem::FsOperation::Read,
            sdk::filesystem::FsOperation::Write,
            sdk::filesystem::FsOperation::Mkdir,
            sdk::filesystem::FsOperation::Rename,
            sdk::filesystem::FsOperation::Remove,
        ]
    }

    fn list(&self, request: &FsRequest<'_>) -> Result<FsListing, sdk::RpcError> {
        fs::list(request)
    }

    fn stat(&self, request: &FsRequest<'_>) -> Result<FsEntry, sdk::RpcError> {
        fs::stat(request)
    }

    fn read(&self, request: &FsRequest<'_>) -> Result<FsRead, sdk::RpcError> {
        fs::read(request)
    }

    fn write(&self, request: &FsRequest<'_>) -> Result<FsWrite, sdk::RpcError> {
        fs::write(request)
    }

    fn mkdir(&self, request: &FsRequest<'_>) -> Result<FsEntry, sdk::RpcError> {
        fs::mkdir(request)
    }

    fn rename(&self, request: &FsRequest<'_>) -> Result<FsEntry, sdk::RpcError> {
        fs::rename(request)
    }

    fn remove(&self, request: &FsRequest<'_>) -> Result<(), sdk::RpcError> {
        fs::remove(request)
    }
}

impl sdk::ConnectionProvider for WebDav {
    fn open(&self, ctx: &sdk::RequestContext) -> Result<serde_json::Value, sdk::RpcError> {
        let mount = ctx.params()["mountId"]
            .as_str()
            .ok_or_else(|| sdk::RpcError::invalid_params("missing mountId"))?;
        Ok(
            serde_json::json!({"kind":"vfs","mountId":mount,"nodeRef":{"mountId":mount,"nodeId":""}}),
        )
    }
    fn validate(&self, ctx: &sdk::RequestContext) -> Result<serde_json::Value, sdk::RpcError> {
        fs::validate(ctx)
    }
}

fn main() {
    let providers = sdk::Providers::new().filesystem(WebDav).connection(WebDav);
    if let Err(e) = providers.serve("org.fiex.webdav") {
        eprintln!("WebDAV IPC failed: {e}");
        std::process::exit(1);
    }
}
