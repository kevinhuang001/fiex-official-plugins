//! OpenList (AList-compatible) native API adapter.
//!
//! Every credential — server, account and timeout — belongs to the connection,
//! so a profile can hold several servers without sharing a login.
use fiex_plugin_sdk::ProviderInvocationExt;
mod client;
mod discovery;
mod fs;
#[path = "../../../support/i18n.rs"]
mod i18n;
mod mutate;
mod sync;

use fiex_plugin_sdk::{
    self as sdk,
    filesystem::{FsEntry, FsListing, FsRead, FsRequest, FsWrite, VfsProvider},
};
use std::sync::Arc;

struct OpenList {
    session: Arc<client::Session>,
}

impl VfsProvider for OpenList {
    fn discover(
        &self,
        ctx: &dyn sdk::ProviderInvocation,
    ) -> Result<Vec<sdk::filesystem::FsOperation>, sdk::RpcError> {
        discovery::discover(ctx, &self.session)
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
        fs::list(request, &self.session)
    }

    fn stat(&self, request: &FsRequest<'_>) -> Result<FsEntry, sdk::RpcError> {
        fs::stat(request, &self.session)
    }

    fn read(&self, request: &FsRequest<'_>) -> Result<FsRead, sdk::RpcError> {
        fs::read(request, &self.session)
    }

    fn write(&self, request: &FsRequest<'_>) -> Result<FsWrite, sdk::RpcError> {
        sync::write(request, &self.session)
    }

    fn mkdir(&self, request: &FsRequest<'_>) -> Result<FsEntry, sdk::RpcError> {
        mutate::mkdir(request, &self.session)
    }

    fn rename(&self, request: &FsRequest<'_>) -> Result<FsEntry, sdk::RpcError> {
        mutate::rename(request, &self.session)
    }

    fn remove(&self, request: &FsRequest<'_>) -> Result<(), sdk::RpcError> {
        mutate::remove(request, &self.session)
    }
}

impl sdk::ConnectionProvider for OpenList {
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
        fs::validate(ctx, &self.session)
    }
}

fn main() {
    let session = Arc::new(client::Session::default());
    let providers = sdk::Providers::new()
        .diagnostics()
        .filesystem(OpenList {
            session: Arc::clone(&session),
        })
        .connection(OpenList { session });
    if let Err(error) = providers.serve("org.fiex.openlist") {
        eprintln!("OpenList IPC failed: transport closed ({:?})", error.kind());
        std::process::exit(1);
    }
}
