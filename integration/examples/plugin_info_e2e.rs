//! Cross-repository test for the 2-column "show info" metadata extension.
#[path = "../support/fixtures.rs"]
mod fixtures;
use fiex_lib::plugins::host as plugin_host;
use serde_json::json;

/// Integration tests install packages directly; record the same install
/// approval a user gives in the marketplace before activating permissions.
fn ensure_approved(id: &str) {
    if !fiex_lib::plugins::host::catalog()
        .iter()
        .any(|plugin| plugin.id == id && plugin.approved)
    {
        fiex_lib::plugins::host::approve_install(id).expect("install approval failed");
    }
}

fn approve(id: &str) {
    let (manifest, _) = plugin_host::installed_manifests()
        .into_iter()
        .find(|(manifest, _)| manifest.id == id)
        .expect("plugin must be installed in FIEX_PLUGIN_DIR");
    ensure_approved(id);
    plugin_host::activate(
        id,
        &plugin_host::activation_requirements(&manifest).unwrap(),
    )
    .unwrap();
}

fn wait_task(task_id: &str) -> serde_json::Value {
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

fn main() {
    fiex_lib::plugins::security::sandbox::run_wrapper_if_requested();
    let root = std::env::temp_dir().join(format!("fiex-info-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("hello.txt");
    std::fs::write(&file, b"abc").unwrap();

    approve("org.fiex.checksum");
    let details =
        fiex_lib::plugins::resolvers::info::file_info_resource(file.to_str().unwrap()).unwrap();
    assert!(
        details.iter().any(|section| section.id == "core.metadata"),
        "built-in metadata section is missing"
    );
    let checksum = details
        .iter()
        .find(|section| section.title == "校验和")
        .expect("checksum info section is missing");
    assert!(checksum.id.starts_with("org.fiex.checksum."));
    assert!(checksum
        .rows
        .iter()
        .any(|row| row.label == "SHA-256" && row.value.starts_with("ba7816bf")));

    approve("org.fiex.archive");
    let archive = root.join("bundle.zip");
    let started = fixtures::invoke_selected(
        "org.fiex.archive",
        "archive.compress",
        json!({
            "paths": [fixtures::reference(std::path::Path::new(&file))],
            "destination": fixtures::reference(std::path::Path::new(&archive)),
            "format": "zip",
            "ignores": []
        }),
        root.to_str().unwrap(),
        &[file.clone(), archive.clone()],
    )
    .expect("compress failed");
    let task_id = started["taskId"].as_str().expect("compress task id");
    let created = wait_task(task_id);
    assert!(
        created["mountId"].is_string() && created["nodeId"].is_string(),
        "archive task result should be a ref: {created}"
    );

    let details =
        fiex_lib::plugins::resolvers::info::file_info_resource(archive.to_str().unwrap()).unwrap();
    let info = details
        .iter()
        .find(|section| section.title == "压缩包")
        .expect("archive info section is missing");
    assert!(info.rows.iter().any(|row| row.label == "文件数"));
    assert!(info.rows.iter().any(|row| row.label == "压缩率"));

    plugin_host::global().stop("org.fiex.archive");
    plugin_host::global().stop("org.fiex.checksum");
    std::fs::remove_dir_all(&root).ok();
    println!("PASS: checksum and archive metadata through the info extension");
}
