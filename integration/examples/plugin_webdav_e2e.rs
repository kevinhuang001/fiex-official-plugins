//! Cross-repository smoke test: real WebDAV plugin through Core's process host,
//! generic listing, virtual entries and bounded stream download.
use fiex_lib::{plugin_host, remote};
use serde_json::json;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

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

fn main() {
    fiex_lib::plugin_sandbox::run_wrapper_if_requested();
    let id = "org.fiex.webdav";
    let entry = plugin_host::catalog()
        .into_iter()
        .find(|plugin| plugin.id == id)
        .expect("WebDAV plugin must be installed in FIEX_PLUGIN_DIR");
    assert!(entry.backend_available);
    let (manifest, _) = plugin_host::installed_manifests()
        .into_iter()
        .find(|(m, _)| m.id == id)
        .unwrap();
    ensure_approved(id);
    plugin_host::activate(
        id,
        &plugin_host::activation_requirements(&manifest).unwrap(),
    )
    .unwrap();

    let info = remote::save_connection(remote::ConnectionInput {
        id: None,
        name: "Mock WebDAV".into(),
        kind: "webdav".into(),
        plugin_id: Some(id.into()),
        settings: [("url".to_string(), json!("http://127.0.0.1:18081/"))]
            .into_iter()
            .collect(),
    })
    .expect("save_connection failed");

    let listing = futures::executor::block_on(remote::list_listing(&info.mount_path, false, &[]))
        .expect("list_listing failed")
        .expect("mount not recognized");
    let names = listing
        .entries
        .iter()
        .map(|entry| entry.name.as_str())
        .collect::<Vec<_>>();
    assert!(names.contains(&"hello.txt"), "{names:?}");
    assert!(names.contains(&"big.bin"), "{names:?}");
    assert!(names.contains(&"sub"), "{names:?}");
    assert!(listing.virtual_volume && listing.read_only);
    assert!(!info.mount_path.is_empty());

    let hello = format!("{}/hello.txt", info.mount_path);
    let options = remote::TransferOptions::default();
    futures::executor::block_on(remote::sync_one(&hello, &options)).expect("download failed");
    assert_eq!(std::fs::read_to_string(&hello).unwrap(), "hello dav");

    let big = format!("{}/big.bin", info.mount_path);
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = cancel.clone();
    let options = remote::TransferOptions {
        cancel: Some(cancel),
        progress: Some(Arc::new(move |done, _total| {
            if done > 0 {
                flag.store(true, Ordering::SeqCst);
            }
        })),
    };
    let error = futures::executor::block_on(remote::sync_one(&big, &options))
        .expect_err("cancel must abort a stream download");
    assert!(error.contains("已取消"), "{error}");
    assert!(
        !std::path::Path::new(&big).exists(),
        "cancelled file must not remain"
    );

    remote::remove_connection(&info.id).expect("remove connection failed");
    plugin_host::global().stop(id);
    println!("PASS: WebDAV plugin listing and bounded cancellable stream download");
}
