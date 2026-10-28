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
use std::sync::{Arc, Mutex};

fn main() {
    let cache = Arc::new(Mutex::new(None));
    if let Err(error) = sdk::Providers::new()
        .info(dispatch::Presentation(Arc::clone(&cache)))
        .preview(dispatch::Presentation(Arc::clone(&cache)))
        .serve_with("org.fiex.data-preview", move |ctx| {
            dispatch::dispatch(ctx, &cache)
        })
    {
        eprintln!("Data preview IPC failed: {error}");
        std::process::exit(1);
    }
}
