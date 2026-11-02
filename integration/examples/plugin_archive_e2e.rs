//! Focused Archive integration test against the real Core host.
//!
//! Reproduces the three user-visible paths that the unit tests cannot:
//! - a context-menu "compress" that creates an archive next to the selection;
//! - a context-menu "extract" that creates the output folder next to it;
//! - a `beforeOpen` hook that consumes the open and extracts in the background.
//!
//! Run with a packaged Archive plugin installed in `FIEX_PLUGIN_DIR`:
//!
//! ```text
//! FIEX_PLUGIN_DIR=/tmp/fiex-plugins \
//!   cargo run --manifest-path integration/Cargo.toml --example plugin_archive_e2e
//! ```
#[path = "../support/fixtures.rs"]
mod fixtures;
use fiex_lib::plugins::host as plugin_host;
use fiex_lib::plugins::resolvers::{context_menu, hooks};
use serde_json::{json, Value};
use std::path::Path;
use std::time::{Duration, Instant};

fn approve(id: &str) {
    let (manifest, _) = plugin_host::installed_manifests()
        .into_iter()
        .find(|(manifest, _)| manifest.id == id)
        .expect("plugin must be installed in FIEX_PLUGIN_DIR");
    // The package is already installed; approve the install snapshot before
    // the activation consent step, exactly like the marketplace flow. A rerun
    // of this test finds the same package already approved.
    if let Err(error) = plugin_host::approve_install(id) {
        assert!(error.contains("already authorized"), "{error}");
    }
    plugin_host::activate(
        id,
        &plugin_host::activation_requirements(&manifest).unwrap(),
    )
    .unwrap();
}

fn plugin_info(id: &str) -> plugin_host::PluginInfo {
    plugin_host::catalog()
        .into_iter()
        .find(|plugin| plugin.id == id)
        .expect("plugin must be installed in FIEX_PLUGIN_DIR")
}

fn wait_task(task_id: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let task = plugin_host::global()
            .task(task_id)
            .expect("Core did not register the plugin task");
        match task.status.as_str() {
            "running" | "cancelling" => {
                assert!(
                    Instant::now() < deadline,
                    "plugin task did not finish: {task:?}"
                );
                std::thread::sleep(Duration::from_millis(50));
            }
            "completed" => return task.result,
            _ => panic!("plugin task failed: {task:?}"),
        }
    }
}

fn wait_until(what: &str, seconds: u64, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < deadline {
        if done() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("timed out waiting for {what}");
}

fn execute_menu(info: &plugin_host::PluginInfo, item: &str, selected: &Path, root: &Path) -> Value {
    let paths = vec![selected.to_string_lossy().into_owned()];
    let menus = context_menu::query_plugins(root.to_str().unwrap(), &paths)
        .unwrap_or_else(|error| panic!("plugin menu query failed: {error}"));
    let menu = menus
        .iter()
        .find(|menu| menu["pluginId"] == info.id)
        .unwrap_or_else(|| panic!("plugin menu for {} was not offered", info.id));
    let session = menu["menuSessionId"]
        .as_str()
        .expect("Core menu session id");
    assert!(
        menu["items"]
            .as_array()
            .is_some_and(|items| { items.iter().any(|value| value["id"] == item) }),
        "plugin menu did not offer {item}: {menu}"
    );
    context_menu::execute(session, item).unwrap_or_else(|error| panic!("{item} failed: {error}"))
}

fn before_open(path: &Path) -> Value {
    hooks::before_open(path.to_str().unwrap())
        .unwrap_or_else(|error| panic!("beforeOpen failed: {error}"))
}

fn main() {
    fiex_lib::plugins::security::sandbox::run_wrapper_if_requested();
    let temp = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(temp.path()).unwrap();
    let file = root.join("hello.txt");
    std::fs::write(&file, b"abc").unwrap();

    approve("org.fiex.archive");
    let info = plugin_info("org.fiex.archive");
    assert_eq!(
        info.ui_timeout_ms, 300_000,
        "manifest extensions.ui.timeoutMs must reach the Core catalog"
    );

    // 1. Context menu "compress": only the source file is selected, so the
    //    archive destination is created next to it (no pre-selected output).
    let started = execute_menu(&info, "archive.zip", &file, &root);
    let task_id = started["taskId"]
        .as_str()
        .expect("compress must return a task id");
    let destination = wait_task(task_id);
    let archive = root.join("hello.zip");
    // Task results are opaque refs, never host paths. The deterministic
    // destination still proves the operation ran on the selected file.
    assert!(
        destination["mountId"].is_string() && destination["nodeId"].is_string(),
        "task result should be the created archive ref: {destination}"
    );
    assert!(
        archive.is_file(),
        "context-menu compress created no archive"
    );

    // 2. Context menu "extract": the output folder is created next to it.
    let started = execute_menu(&info, "archive.extract", &archive, &root);
    let task_id = started["taskId"]
        .as_str()
        .expect("extract must return a task id");
    let applied = wait_task(task_id);
    assert!(
        applied["mountId"].is_string() && applied["nodeId"].is_string(),
        "extract task result should be a folder ref: {applied}"
    );
    let applied = root.join("hello");
    assert!(
        applied.is_dir(),
        "context-menu extract created no folder: {}",
        applied.display()
    );
    assert_eq!(
        std::fs::read(applied.join("hello.txt")).expect("extracted file"),
        b"abc"
    );

    // 2b. The advanced-compress panel names its output folder in the same
    //     invocation, so Core grants that folder as an explicit candidate.
    let panel_archive = root.join("panel.zip");
    let started = fixtures::invoke_selected(
        "org.fiex.archive",
        "archive.compress",
        json!({
            "paths": [fixtures::reference(std::path::Path::new(&file))],
            "root": fixtures::reference(std::path::Path::new(&root)),
            "destinationName": "panel.zip",
            "format": "zip",
            "ignores": [],
        }),
        root.to_str().unwrap(),
        &[file.clone(), root.clone()],
    )
    .expect("advanced panel compress failed");
    wait_task(started["taskId"].as_str().expect("panel task id"));
    assert!(panel_archive.is_file(), "advanced panel created no archive");

    // 3. beforeOpen consumes the OS open, returns handled:true immediately and
    //    never leaks a task payload. Extraction happens in the background.
    let hooked = root.join("hooked.zip");
    std::fs::copy(&archive, &hooked).unwrap();
    let response = before_open(&hooked);
    assert_eq!(response["handled"], true, "hook must consume the open");
    assert!(
        response.get("taskId").is_none(),
        "hook response must not expose the background task id: {response}"
    );
    assert_eq!(
        response,
        json!({"handled": true}),
        "hook response must stay a bare handled flag"
    );
    let extracted = root.join("hooked").join("hello.txt");
    wait_until("background hook extraction", 120, || extracted.is_file());
    assert_eq!(std::fs::read(&extracted).unwrap(), b"abc");

    // 4. A broken archive is still consumed and its failure reaches the plugin
    //    error channel instead of falling through to the OS handler.
    let broken = root.join("broken.zip");
    std::fs::write(&broken, b"this is not a zip").unwrap();
    fiex_lib::plugins::errors::clear("org.fiex.archive");
    let response = before_open(&broken);
    assert_eq!(response["handled"], true);
    assert!(response.get("taskId").is_none());
    wait_until("hook failure report", 30, || {
        !fiex_lib::plugins::errors::list("org.fiex.archive").is_empty()
    });

    // 5. The negotiated budget is part of the preview API response, so the
    //    preview document can align its own request tim