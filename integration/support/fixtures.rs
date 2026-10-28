use fiex_lib::plugins::{
    contracts::ResourceRef,
    host,
    resource::resource_context::{ResourceBinding, ResourceSnapshot},
};
use serde_json::Value;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};
fn snapshots() -> &'static Mutex<HashMap<PathBuf, ResourceRef>> {
    static VALUES: OnceLock<Mutex<HashMap<PathBuf, ResourceRef>>> = OnceLock::new();
    VALUES.get_or_init(Default::default)
}
pub fn reference(path: &Path) -> ResourceRef {
    snapshots()
        .lock()
        .unwrap()
        .entry(path.to_path_buf())
        .or_insert_with(|| ResourceSnapshot::new(&path.to_string_lossy(), path).reference)
        .clone()
}
pub fn invoke_selected(
    plugin: &str,
    method: &str,
    params: Value,
    directory: &str,
    selected: &[PathBuf],
) -> Result<Value, String> {
    let bindings = selected
        .iter()
        .map(|path| ResourceBinding {
            reference: reference(path),
            path: path.to_string_lossy().into_owned(),
        })
        .collect::<Vec<_>>();
    host::global()
        .invoke_selected_with_bindings(plugin, method, params, directory, selected, &bindings)
}
