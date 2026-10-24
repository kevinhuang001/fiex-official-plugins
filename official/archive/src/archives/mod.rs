//! Compression engines run in the plugin; every file operation uses the SDK.
mod compress;
mod extract;
mod formats;
mod paths;

pub use compress::compress;
pub use extract::extract;
pub use paths::default_destination;

const MAX_TOTAL: u64 = 20 * 1024 * 1024 * 1024;
const MAX_ENTRIES: usize = 100000;

fn err(error: impl ToString) -> String {
    error.to_string()
}
