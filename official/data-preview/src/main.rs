//! Table-structured data preview: SQLite, CSV/TSV, XLSX/XLS, Parquet, Arrow
//! IPC and Avro. Everything is paged; binary formats are parsed with their
//! format crates, never by hand.
mod columnar;
mod dispatch;
mod open;
mod sheet;
mod sqlite;
mod table;
mod tabular;

use fiex_plugin_sdk as sdk;
use std::sync::Mutex;

fn main() {
    let cache = Mutex::new(None);
    if let Err(error) = sdk::serve("org.fiex.data-preview", move |ctx| {
        dispatch::dispatch(ctx, &cache)
    }) {
        eprintln!("Data preview IPC failed: {error}");
        std::process::exit(1);
    }
}
