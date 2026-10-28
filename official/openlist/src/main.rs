//! OpenList (AList-compatible) native API adapter.
//!
//! Every credential — server, account and timeout — belongs to the connection,
//! so a profile can hold several servers without sharing a login.
mod client;
mod discovery;
mod fs;
#[path = "../../../support/i18n.rs"]
mod i18n;
mod mutate;
mod sync;

use fiex_plugin_sdk::{
    self as sdk,
    filesystem::{FSProvider, FsEntry, FsListing, FsRead, FsRequest, FsWrite},
};
use std::sync::Arc;

struct OpenList {
    session: Arc<client::Session>,
}

impl FSProvider for OpenList {
    fn discover(
        &self,
        ctx: &sdk::RequestContext,
    ) -> Result<Vec<sdk::filesystem::FsOp>, sdk::RpcError> {
        discovery::discover(ctx, &self.session)
    }

    fn operations(&self) -> &'static [sdk::filesystem::FsOp] {
        &[
            sdk::filesystem::FsOp::List,
            sdk::filesystem::FsOp::Stat,
            sdk::filesystem::FsOp::Read,
            sdk::filesystem::FsOp::Write,
            sdk::filesystem::FsOp::Mkdir,
            sdk::filesystem::FsOp::Rename,
            sdk::filesystem::FsOp::Remove,
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
    fn open(&self, ctx: &sdk::RequestContext) -> Result<serde_json::Value, sdk::RpcError> {
        let mount = ctx.params()["mountId"]
            .as_str()
            .ok_or_else(|| sdk::RpcError::invalid_params("missing mountId"))?;
        Ok(
            serde_json::json!({"kind":"vfs","mountId":mount,"nodeRef":{"mountId":mount,"nodeId":""}}),
        )
    }
    fn validate(&self, ctx: &sdk::RequestContext) -> Result<serde_json::Value, sdk::RpcError> {
        fs::validate(ctx, &self.session)
    }
}

fn main() {
    let session = Arc::new(client::Session::default());
    let providers = sdk::Providers::new()
        .filesystem(OpenList {
            session: Arc::clone(&session),
        })
        .connection(OpenList { session });
    if let Err(error) = providers.serve("org.fiex.openlist") {
        eprintln!("OpenList IPC failed: {error}");
        std::process::exit(1);
    }
}
