//! Standalone WebDAV adapter. The Fiex host owns mount configuration,
//! credential storage, cache, sync rules and conflict resolution.
mod client;
mod commands;
mod fs;
mod sync;

use fiex_plugin_sdk as sdk;
use std::{
    collections::HashMap,
    sync::{atomic::AtomicU64, Arc, Mutex},
};

fn main() {
    let streams: fs::Streams = Arc::new(Mutex::new(HashMap::new()));
    let next = AtomicU64::new(0);
    if let Err(e) = sdk::serve("org.fiex.webdav", move |ctx| {
        commands::dispatch(ctx, &streams, &next)
    }) {
        eprintln!("WebDAV IPC failed: {e}");
        std::process::exit(1);
    }
}
