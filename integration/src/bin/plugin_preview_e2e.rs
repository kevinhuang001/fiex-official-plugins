//! Cross-repository test for the unified EventContext/PreviewView contract.
#[path = "../../support/fixtures.rs"]
mod fixtures;
use fiex_lib::plugins::{
    events, host as plugin_host, providers::preview, resource::resource_context::ResourceSnapshot,
};
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
    fiex_lib::plugins::security::sandbox::run_wrapper_if_requested();
    let root = std::env::temp_dir().join(format!("fiex-preview-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("hello.txt");
    std::fs::write(&source, b"hello preview\n").unwrap();
    let archive = root.join("bundle.zip");

    approve("org.fiex.archive");
    let started = fixtures::invoke_selected(
        "org.fiex.archive",
        "archive.compress",
        json!({
            "paths": [fixtures::reference(std::path::Path::new(&source))],
            "destination": fixtures::reference(std::path::Path::new(&archive)),
            "format": "zip",
            "ignores": []
        }),
        root.to_str().unwrap(),
        &[source.clone(), archive.clone(), root.clone()],
    )
    .expect("compress failed");
    wait_task(started["taskId"].as_str().expect("compress task id"));

    let display = archive.to_str().unwrap();
    let candidates = preview::resource_candidates_json(display).expect("preview candidates");
    assert!(
        candidates
            .iter()
            .any(|candidate| candidate["id"] == "plugin:org.fiex.archive"),
        "archive preview provider is missing: {candidates:?}"
    );

    let plugin = plugin_host::catalog()
        .into_iter()
        .find(|plugin| plugin.id == "org.fiex.archive")
        .expect("archive plugin must be available");
    let resource = ResourceSnapshot::new(display, &archive, "local", "application/zip");
    // Prepare preview.open through Core's trusted event boundary. It creates
    // the user-action context and the scoped resource grant for this target.
    let (context, scope) = events::resource_scope(
        &plugin,
        "preview.open",
        json!({"sessionId": "integration-preview-session"}),
        std::slice::from_ref(&resource),
        None,
    )
    .expect("preview event context");
    assert_eq!(context["targets"][0]["metadata"]["name"], "bundle.zip");
    assert_eq!(context["targets"][0]["metadata"]["extension"], "zip");
    assert!(context["targets"][0]["ref"]["mountId"].is_string());
    assert!(context["targets"][0]["ref"]["nodeId"].is_string());
    assert_eq!(context["sessionId"], "integration-preview-session");
    assert!(context.get("resource").is_none());
    assert!(context.get("intent").is_none());
    assert!(context["targets"][0].get("read").is_none());
    let view = events::invoke_prepared(&plugin.id, "preview.open", context, scope)
        .expect("archive preview.open failed");
    assert_eq!(view["kind"], "html", "{view}");
    assert_eq!(view["view"]["source"], "package", "{view}");
    let entry = view["view"]["entry"]
        .as_str()
        .expect("package preview entry");
    let html_path =
        plugin_host::plugin_view_asset("org.fiex.archive", entry).expect("package preview asset");
    let html = std::fs::read_to_string(html_path).expect("read package preview asset");
    assert!(html.to_ascii_lowercase().contains("<html"), "{entry}");

    let text = root.join("hello.txt");
    let text_candidates =
        preview::resource_candidates_json(text.to_str().unwrap()).expect("text preview candidates");
    assert!(
        text_candidates
            .iter()
            .any(|candidate| candidate["id"] == "builtin:text"),
        "built-in text preview is missing: {text_candidates:?}"
    );

    let pdf = root.join("sample.pdf");
    std::fs::write(
        &pdf,
        b"%PDF-1.4\n1 0 obj << /Type /Catalog >> endobj\ntrailer << /Root 1 0 R >>\n%%EOF\n",
    )
    .unwrap();
    let pdf_candidates =
        preview::resource_candidates_json(pdf.to_str().unwrap()).expect("PDF preview candidates");
    assert!(
        pdf_candidates
            .iter()
            .any(|candidate| candidate["id"] == "builtin:pdf"),
        "built-in PDF preview is missing: {pdf_candidates:?}"
    );

    plugin_host::global().stop("org.fiex.archive");
    std::fs::remove_dir_all(&root).ok();
    println!("PASS: Core preview routing and scoped PreviewProvider invocation");
}
