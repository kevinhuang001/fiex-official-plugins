//! OpenList (AList-compatible) native API adapter.
//!
//! Every credential — server, account and timeout — belongs to the connection,
//! so a profile can hold several servers without sharing a login.
mod client;
mod commands;
mod fs;
mod mutate;
mod sync;

use fiex_plugin_sdk::{
    self as sdk,
    filesystem::{FileSystemProvider, FsEntry, FsListing, FsRead, FsRequest, FsWrite},
};
use std::sync::Arc;

struct OpenList {
    session: Arc<client::Session>,
}

#[sdk::filesystem]
impl FileSystemProvider for OpenList {
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

fn main() {
    let session = Arc::new(client::Session::default());
    let providers = sdk::Providers::new().filesystem(OpenList {
        session: Arc::clone(&session),
    });
    if let Err(error) = providers.serve_with("org.fiex.openlist", move |ctx| {
        commands::dispatch(ctx, &session)
    }) {
        eprintln!("OpenList IPC failed: {error}");
        std::process::exit(1);
    }
}
