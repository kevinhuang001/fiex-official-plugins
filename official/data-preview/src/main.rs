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
use std::cell::RefCell;

fn main() {
    let cache = RefCell::new(None);
    if let Err(error) =
        sdk::serve_with_host("org.fiex.data-preview", move |method, params, host| {
            dispatch::dispatch(method, params, &cache, host)
        })
    {
        eprintln!("Data preview IPC failed: {error}");
        std::process::exit(1);
    }
}
