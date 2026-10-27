//! Standalone WebDAV adapter. The Fiex host owns mount configuration,
//! credential storage, cache, sync rules and conflict resolution.
mod client;
mod commands;
mod fs;

use fiex_plugin_sdk::{
    self as sdk,
    filesystem::{FileSystemProvider, FsEntry, FsListing, FsRead, FsRequest, FsWrite},
};

struct WebDav;

#[sdk::filesystem]
impl FileSystemProvider for WebDav {
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

fn main() {
    let providers = sdk::Providers::new().filesystem(WebDav);
    if let Err(e) = providers.serve_with("org.fiex.webdav", commands::dispatch) {
        eprintln!("WebDAV IPC failed: {e}");
        std::process::exit(1);
    }
}
