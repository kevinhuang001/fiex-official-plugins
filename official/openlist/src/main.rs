//! OpenList (AList-compatible) native API adapter.
//!
//! Credentials live in the plugin's settings, not in the connection: one
//! account per plugin instance.
mod client;
mod commands;
mod fs;
mod sync;

use fiex_plugin_sdk as sdk;
use std::sync::Arc;

fn main() {
    let session = Arc::new(client::Session::default());
    let reads = Arc::new(fs::Reads::default());
    if let Err(error) = sdk::serve("org.fiex.openlist", move |ctx| {
        commands::dispatch(ctx, &session, &reads)
    }) {
        eprintln!("OpenList IPC failed: {error}");
        std::process::exit(1);
    }
}
