//! Standalone WebDAV adapter. The Fiex host owns mount configuration,
//! credential storage, cache, sync rules and conflict resolution.
use fiex_plugin_sdk::ProviderInvocationExt;
mod client;
mod discovery;
mod fs;
#[path = "../../../support/i18n.rs"]
mod i18n;

use fiex_plugin_sdk::{
    self as sdk,
    filesystem::{FsEntry, FsListing, FsRead, FsRequest, FsWrite, VfsProvider},
};

struct WebDav;

impl VfsProvider for WebDav {
    fn discover(
        &self,
        ctx: &dyn sdk::ProviderInvocation,
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
    fn open(&self, ctx: &dyn sdk::ProviderInvocation) -> Result<sdk::ConnectionOpenResult, sdk::RpcError> {
        let mount = ctx
            .input()?
            .connection_request()
            .and_then(|request| request.mount_id.as_deref())
            .ok_or_else(|| sdk::RpcError::invalid_params("missing mountId"))?;
        Ok(sdk::ConnectionOpenResult::Vfs {
            mount_id: mount.into(),
            node_ref: sdk::ResourceRef {
                mount_id: mount.into(),
                node_id: String::new(),
            },
        })
    }
    fn validate(&self, ctx: &dyn sdk::ProviderInvocation) -> Result<sdk::ConnectionValidation, sdk::RpcError> {
        fs::validate(ctx)
    }
}

fn main() {
    let providers = sdk::Providers::new()
        .diagnostics()
        .filesystem(WebDav)
        .connection(WebDav);
    if let Err(e) = providers.serve("org.fiex.webdav") {
        eprintln!("WebDAV IPC failed: transport closed ({:?})", e.kind());
        std::process::exit(1);
    }
}
