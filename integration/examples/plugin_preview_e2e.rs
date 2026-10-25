//! Cross-repository test for the standard PreviewContext/PreviewView contract.
use fiex_lib::plugin_host;
use serde_json::json;

fn approve(id: &str) {
    let (manifest, _) = plugin_host::installed_manifests()
        .into_iter()
        .find(|(manifest, _)| manifest.id == id)
        .expect("plugin must be installed in FIEX_PLUGIN_DIR");
    plugin_host::activate(
        id,
        &plugin_host::activation_requirements(&manifest).unwrap(),
    )
    .unwrap();
}

fn wait_task(task_id: &str) {
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
            "completed" => return,
            _ => panic!("plugin task failed: {task:?}"),
        }
    }
}

fn main() {
    fiex_lib::plugin_sandbox::run_wrapper_if_requested();
    let root = std::env::temp_dir().join(format!("fiex-preview-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("hello.txt");
    std::fs::write(&source, b"hello preview\n").unwrap();
    let archive = root.join("bundle.zip");

    approve("org.fiex.archive");
    let started = plugin_host::global()
        .invoke_selected(
            "org.fiex.archive",
            "archive.compress",
            json!({
                "paths": [source],
                "destination": archive,
                "format": "zip",
                "ignores": []
            }),
            root.to_str().unwrap(),
            &[source.clone(), archive.clone()],
        )
        .expect("compress failed");
    wait_task(started["taskId"].as_str().expect("compress task id"));

    let opened = fiex_lib::plugin_preview::open(
        archive.to_str().unwrap(),
        archive.to_str().unwrap(),
        "org.fiex.archive",
        "archive",
    )
    .expect("archive preview.open failed");
    assert_eq!(opened["view"]["kind"], "html", "{opened}");
    let html = opened["view"]["html"].as_str().unwrap();
    assert!(html.to_ascii_lowercase().contains("<html"), "{opened}");
    let context = &opened["context"];
    assert_eq!(context["resource"]["metadata"]["name"], "bundle.zip");
    assert_eq!(context["resource"]["metadata"]["extension"], "zip");
    assert!(context["resource"]["read"]["resourceId"].is_string());
    assert_eq!(context["intent"], "preview");
    let (directory, paths) = fiex_lib::plugin_preview::session_scope(
        opened["sessionId"].as_str().unwrap(),
        "org.fiex.archive",
    )
    .expect("preview session scope");
    assert_eq!(std::path::Path::new(&directory), root);
    assert_eq!(paths, vec![archive.clone()]);

    // Without files.read Core must not project a read capability.
    let (manifest, _) = plugin_host::installed_manifests()
        .into_iter()
        .find(|(manifest, _)| manifest.id == "org.fiex.archive")
        .unwrap();
    let declared = plugin_host::declared_permissions(&manifest).unwrap();
    assert!(declared.contains("files.read"));
    assert!(fiex_lib::plugin_preview::open(
        archive.to_str().unwrap(),
        archive.to_str().unwrap(),
        "org.fiex.archive",
        "missing-provider",
    )
    .is_err());

    plugin_host::global().stop("org.fiex.archive");
    std::fs::remove_dir_all(&root).ok();
    println!("PASS: standard PreviewContext/PreviewView and scoped preview session");
}
