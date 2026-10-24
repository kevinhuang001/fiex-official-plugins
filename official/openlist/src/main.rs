//! OpenList (AList-compatible) native API adapter.
//!
//! Credentials live in the plugin's settings, not in the connection: one
//! account per plugin instance.
mod client;
mod commands;
mod fs;

use fiex_plugin_sdk as sdk;
use std::rc::Rc;

fn main() {
    let session = Rc::new(client::Session::default());
    let reads = Rc::new(fs::Reads::default());
    if let Err(error) = sdk::serve_with_host("org.fiex.openlist", move |method, params, host| {
        commands::dispatch(method, params, &session, &reads, host)
    }) {
        eprintln!("OpenList IPC failed: {error}");
        std::process::exit(1);
    }
}
