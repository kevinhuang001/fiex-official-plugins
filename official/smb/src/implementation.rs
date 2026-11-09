//! SMB provider organized by transport, connection, listing, mutations, and reads.
use fiex_plugin_sdk::ProviderInvocationExt;
use fiex_plugin_sdk::{
    self as sdk,
    filesystem::{FsEntry, FsListing, FsOperation, FsRead, FsRequest, FsWrite, VfsProvider},
    RpcError,
};
use smb::{GetLen, ReadAt};
use std::{io, str::FromStr};

#[path = "connection.rs"]
mod connection;
#[path = "discovery.rs"]
mod discovery;
#[path = "listing.rs"]
mod listing;
#[path = "mutation.rs"]
mod mutation;
#[path = "provider.rs"]
mod provider;
#[path = "reading.rs"]
mod reading;
#[path = "sync.rs"]
mod sync;
#[path = "transport.rs"]
mod transport;

use connection::*;
use listing::*;
use mutation::*;
use reading::*;

pub(crate) fn run() {
    provider::run();
}
