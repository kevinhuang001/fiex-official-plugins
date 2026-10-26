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
use fiex_lib::plugin_host;
use fiex_lib::plugin_resource_context::ResourceSnapshot;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
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
    let resource = ResourceSnapshot::new(selected.to_str().unwrap(), selected);
    fiex_lib::plugin_events::call_owner(
        "contextMenu.execute",
        &info.id,
        json!({
            "menuSessionId": "archive-e2e",
            "itemId": item,
            "interactionId": "archive-e2e",
            "paths": [selected],
            "path": selected,
            "root": root,
        }),
        std::slice::from_ref(&resource),
        Some(root.to_str().unwrap()),
    )
    .unwrap_or_else(|error| panic!("{item} failed: {error}"))
}

fn before_open(info: &plugin_host::PluginInfo, path: &Path) -> Value {
    let resource = ResourceSnapshot::new(path.to_str().unwrap(), path);
    fiex_lib::plugin_events::invoke_resources(
        info,
        "hooks.invoke",
        json!({"event": "beforeOpen"}),
        std::slice::from_ref(&resource),
        None,
    )
    .unwrap_or_else(|error| panic!("beforeOpen failed: {error}"))
}

fn main() {
    fiex_lib::plugin_sandbox::run_wrapper_if_requested();
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
    assert_eq!(
        destination.as_str(),
        Some(archive.to_string_lossy().as_ref()),
        "task result should be the created archive"
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
    let applied = PathBuf::from(applied.as_str().expect("extract target"));
    assert!(
        applied.is_dir(),
        "context-menu extract created no folder: {}",
        applied.display()
    );
    assert_eq!(
        std::fs::read(applied.join("hello.txt")).expect("extracted file"),
        b"abc"
    );
    assert!(root.join("hello").is_dir());

    // 2b. The advanced-compress panel names its output folder in the same
    //     invocation, so Core grants that folder as an explicit candidate.
    let panel_archive = root.join("panel.zip");
    let started = plugin_host::global()
        .invoke_selected(
            "org.fiex.archive",
            "archive.compress",
            json!({
                "paths": [file],
                "root": root,
                "destination": panel_archive,
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
    let response = before_open(&info, &hooked);
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
    fiex_lib::plugin_errors::clear("org.fiex.archive");
    let response = before_open(&info, &broken);
    assert_eq!(response["handled"], true);
    assert!(response.get("taskId").is_none());
    wait_until("hook failure report", 30, || {
        !fiex_lib::plugin_errors::list("org.fiex.archive").is_empty()
    });

    // 5. The negotiated budget is part of the preview API response, so the
    //    preview document can align its own request timer with Core.
    let preview = fiex_lib::plugin_preview::open_resource(
        archive.to_str().unwrap(),
        Some("plugin:org.fiex.archive"),
        None,
    )
    .expect("archive preview must resolve");
    assert_eq!(preview["view"]["kind"], "html");
    assert_eq!(preview["view"]["timeoutMs"], 300_000);
    assert!(preview["view"]["html"]
        .as_str()
        .unwrap()
        .contains("archive.list"));

    // The preview document lists through the same Core host.
    let listing = plugin_host::global()
        .invoke_selected(
            "org.fiex.archive",
            "archive.list",
            json!({"path": archive}),
            root.to_str().unwrap(),
            std::slice::from_ref(&archive),
        )
        .expect("archive.list through the host");
    assert_eq!(listing["supported"], true);
    assert!(listing["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["name"] == "hello.txt"));

    plugin_host::global().stop("org.fiex.archive");
    println!("PASS: Archive context-menu and beforeOpen flows through the real Core host");
}
