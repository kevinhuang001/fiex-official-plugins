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

use fiex_plugin_sdk::{Files, RequestContext, TaskContext};

pub(crate) const CANCELLED: &str = "archive.cancelled";

/// The file and lifecycle view shared by synchronous requests and background
/// tasks. Progress and cancellation are no-ops for a synchronous request.
pub(crate) trait ArchiveContext {
    fn files(&self) -> Files;
    fn progress(&self, done: u64, total: Option<u64>);
    fn cancelled(&self) -> bool;
}

impl ArchiveContext for RequestContext {
    fn files(&self) -> Files {
        RequestContext::files(self)
    }
    fn progress(&self, _done: u64, _total: Option<u64>) {}
    fn cancelled(&self) -> bool {
        RequestContext::is_cancelled(self)
    }
}

impl ArchiveContext for TaskContext {
    fn files(&self) -> Files {
        TaskContext::files(self)
    }
    fn progress(&self, done: u64, total: Option<u64>) {
        TaskContext::progress(self, done, total);
    }
    fn cancelled(&self) -> bool {
        TaskContext::is_cancelled(self)
    }
}

fn err(error: impl ToString) -> String {
    error.to_string()
}
