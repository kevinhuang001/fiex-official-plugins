//! Cross-repository smoke test for on-demand Info fields through Core's plugin host.
#[path = "../../support/fixtures.rs"]
mod fixtures;
use fiex_lib::plugins::host as plugin_host;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// Integration tests install packages directly; record the same install
/// approval a user gives in the marketplace before activating permissions.
fn approve(id: &str) {
    let (manifest, _) = plugin_host::installed_manifests()
        .into_iter()
        .find(|(manifest, _)| manifest.id == id)
        .expect("plugin must be installed in FIEX_PLUGIN_DIR");
    if !plugin_host::catalog()
        .iter()
        .any(|plugin| plugin.id == id && plugin.approved)
    {
        plugin_host::approve_install(id).expect("install approval failed");
    }
    plugin_host::activate(
        id,
        &plugin_host::activation_requirements(&manifest).unwrap(),
    )
    .unwrap();
}

fn wait_task(task_id: &str) -> Value {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    loop {
        let task = plugin_host::global()
            .task(task_id)
            .expect("Core did not register the plugin task");
        match task.status.as_str() {
            "running" | "cancelling" => {
                assert!(
                    std::time::Instant::now() < deadline,
                    "plugin task did not finish: {task:?}"
                );
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            "completed" => return task.result,
            _ => panic!("plugin task failed: {task:?}"),
        }
    }
}

fn compute(plugin: &str, root: &Path, targets: &[PathBuf], fields: Value) -> Value {
    fixtures::invoke_selected(
        plugin,
        "info.compute",
        json!({ "fields": fields }),
        root.to_str().unwrap(),
        targets,
    )
    .expect("info.compute failed")
}

fn field<'a>(result: &'a Value, field_id: &str) -> &'a Value {
    result["fields"]
        .as_array()
        .expect("info.compute must return fields")
        .iter()
        .find(|field| field["fieldId"].as_str() == Some(field_id))
        .unwrap_or_else(|| panic!("field {field_id} is missing: {result}"))
}

fn main() {
    fiex_lib::plugins::security::sandbox::run_wrapper_if_requested();
    let root = std::env::temp_dir().join(format!("fiex-info-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("hello.txt");
    std::fs::write(&file, b"abc").unwrap();

    approve("org.fiex.checksum");
    let checksum = compute(
        "org.fiex.checksum",
        &root,
        std::slice::from_ref(&file),
        json!([{ "sectionId": "checksums", "fieldId": "sha256" }]),
    );
    assert_eq!(
        field(&checksum, "sha256")["value"],
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );

    approve("org.fiex.archive");
    let archive = root.join("bundle.zip");
    let started = fixtures::invoke_selected(
        "org.fiex.archive",
        "archive.compress",
        json!({
            "paths": [fixtures::reference(&file)],
            "destination": fixtures::reference(&archive),
            "format": "zip",
            "ignores": []
        }),
        root.to_str().unwrap(),
        &[file.clone(), archive.clone(), root.clone()],
    )
    .expect("compress failed");
    let task_id = started["taskId"].as_str().expect("compress task id");
    let created = wait_task(task_id);
    assert!(
        created["mountId"].is_string() && created["nodeId"].is_string(),
        "archive task result should be a ref: {created}"
    );

    let archive_info = compute(
        "org.fiex.archive",
        &root,
        std::slice::from_ref(&archive),
        json!([
            { "sectionId": "archive", "fieldId": "format" },
            { "sectionId": "archive", "fieldId": "entryCount" }
        ]),
    );
    assert_eq!(field(&archive_info, "entryCount")["value"], "1");
    assert!(
        field(&archive_info, "format")["value"]
            .as_str()
            .is_some_and(|value| !value.is_empty()),
        "archive format should be available: {archive_info}"
    );

    plugin_host::global().stop("org.fiex.archive");
    plugin_host::global().stop("org.fiex.checksum");
    std::fs::remove_dir_all(&root).ok();
    println!("PASS: checksum and archive on-demand Info fields through Core's plugin host");
}
