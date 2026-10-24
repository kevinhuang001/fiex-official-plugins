//! Standalone WebDAV adapter. The Fiex host owns mount configuration,
//! credential storage, cache, sync rules and conflict resolution.
mod client;
mod commands;
mod fs;

use fiex_plugin_sdk as sdk;
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};

fn main() {
    let streams: fs::Streams = Rc::new(RefCell::new(HashMap::new()));
    let next = Cell::new(0);
    if let Err(e) = sdk::serve_with_host("org.fiex.webdav", move |method, p, host| {
        commands::dispatch(method, p, &streams, &next, host)
    }) {
        eprintln!("WebDAV IPC failed: {e}");
        std::process::exit(1);
    }
}
