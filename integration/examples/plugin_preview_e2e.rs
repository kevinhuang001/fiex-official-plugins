//! Cross-repository test for the unified EventContext/PreviewView contract.
use fiex_lib::plugin_host;
use serde_json::json;

/// Integration tests install packages directly; record the same install
/// approval a user gives in the marketplace before activating permissions.
fn ensure_approved(id: &str) {
    if !fiex_lib::plugin_host::catalog()
        .iter()
        .any(|plugin| plugin.id == id && plugin.approved)
    {
        fiex_lib::plugin_host::approve_install(id).expect("install approval failed");
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
        "org.fiex.archive",
    )
    .expect("archive preview.open failed");
    assert_eq!(opened["view"]["kind"], "html", "{opened}");
    let html = opened["view"]["html"].as_str().unwrap();
    assert!(html.to_ascii_lowercase().contains("<html"), "{opened}");
    let context = &opened["context"];
    // Preview uses the unified EventContext. A ResourceRef is an opaque
    // operation-scoped identity; the old resource/read/intent envelope is gone.
    assert_eq!(context["targets"][0]["metadata"]["name"], "bundle.zip");
    assert_eq!(context["targets"][0]["metadata"]["extension"], "zip");
    assert!(context["targets"][0]["ref"]["mountId"].is_string());
    assert!(context["targets"][0]["ref"]["nodeId"].is_string());
    assert!(context["sessionId"].as_str().is_some_and(|id| !id.is_empty()));
    assert!(context.get("resource").is_none());
    assert!(context.get("intent").is_none());
    assert!(context["targets"][0].get("read").is_none());
    let unified = fiex_lib::plugin_preview::open_provider(
        archive.to_str().unwrap(),
        archive.to_str().unwrap(),
        Some("plugin:org.fiex.archive"),
    )
    .expect("unified plugin preview failed");
    assert_eq!(unified["provider"], "压缩与解压");
    assert_eq!(unified["view"]["kind"], "html", "{unified}");
    assert_eq!(unified["view"]["pluginId"], "org.fiex.archive");

    let text = root.join("hello.txt");
    let builtin = fiex_lib::plugin_preview::open_provider(
        text.to_str().unwrap(),
        text.to_str().unwrap(),
        Some("builtin:text"),
    )
    .expect("builtin text preview failed");
    assert_eq!(builtin["view"]["kind"], "text");
    assert_eq!(builtin["provider"], "Fiex");
    assert!(builtin["view"]["text"]
        .as_str()
        .unwrap()
        .contains("hello preview"));

    let pdf = root.join("sample.pdf");
    std::fs::write(
        &pdf,
        b"%PDF-1.4\n1 0 obj << /Type /Catalog >> endobj\ntrailer << /Root 1 0 R >>\n%%EOF\n",
    )
    .unwrap();
    let pdf_view = fiex_lib::plugin_preview::open_provider(
        pdf.to_str().unwrap(),
        pdf.to_str().unwrap(),
        Some("builtin:pdf"),
    )
    .expect("builtin pdf preview failed");
    assert_eq!(pdf_view["view"]["kind"], "pdf");

    let scope = fiex_lib::plugin_preview::session_scope(
        opened["sessionId"].as_str().unwrap(),
        "org.fiex.archive",
    )
    .expect("preview session scope");
    assert_eq!(scope.path(), root);
    assert_eq!(
        scope
            .resolve_reference(&context["targets"][0]["ref"])
            .unwrap(),
        archive.to_str().unwrap()
    );

    // Resource identity does not grant reads; the broker checks FilesRead per call.
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
