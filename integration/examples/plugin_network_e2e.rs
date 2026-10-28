//! Real HTTP and TCP SDK calls through Core, including explicit host consent.
use base64::{engine::general_purpose::STANDARD, Engine};
use fiex_lib::plugins::host as plugin_host;
use serde_json::{json, Value};

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

fn check(id: &str, config: Value, expected: &[u8]) {
    let (manifest, _) = plugin_host::installed_manifests()
        .into_iter()
        .find(|(manifest, _)| manifest.id == id)
        .expect("network plugin must be installed");
    ensure_approved(id);
    plugin_host::activate(
        id,
        &plugin_host::activation_requirements(&manifest).unwrap(),
    )
    .unwrap();
    // Neither loopback nor LAN addresses have implicit access.
    assert!(plugin_host::global()
        .invoke(id, "connection.validate", json!({"connection": config}))
        .is_err());
    plugin_host::set_network_domain(id, "127.0.0.1", true).unwrap();
    let listing = plugin_host::global()
        .invoke(
            id,
            "fs.list",
            json!({"connection": config, "directoryId": "", "limit": 200}),
        )
        .expect("network listing through Core failed");
    assert!(listing["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["name"] == "hello.txt"));
    let opened = plugin_host::global()
        .invoke(
            id,
            "fs.openRead",
            json!({"connection": config, "nodeId": "hello.txt"}),
        )
        .unwrap();
    let stream = opened["streamId"].as_str().unwrap();
    let result = plugin_host::global()
        .invoke(
            id,
            "fs.readStream",
            json!({"streamId": stream, "offset": 0, "maxBytes": 262144}),
        )
        .unwrap();
    assert_eq!(
        STANDARD.decode(result["base64"].as_str().unwrap()).unwrap(),
        expected
    );
    assert_eq!(result["eof"], true);
    plugin_host::global()
        .invoke(id, "fs.closeRead", json!({"streamId": stream}))
        .unwrap();
    let source_dir = tempfile::tempdir().unwrap();
    let source = source_dir.path().join("source.dat");
    let edited = b"offline edit".repeat(50000);
    for contents in [edited.as_slice(), expected] {
        std::fs::write(&source, contents).unwrap();
        let baseline = plugin_host::global()
            .invoke(
                id,
                "fs.stat",
                json!({"connection":config,"nodeId":"hello.txt"}),
            )
            .unwrap();
        let begin = plugin_host::global()
            .invoke(
                id,
                "fs.beginWrite",
                json!({"connection":config,"nodeId":"hello.txt","size":contents.len(),"ifRevision":baseline["revision"]}),
            )
            .unwrap();
        let stream = begin["streamId"].as_str().unwrap().to_string();
        let mut offset = 0usize;
        for chunk in contents.chunks(256 * 1024) {
            plugin_host::global()
                .invoke(
                    id,
                    "fs.writeStream",
                    json!({"streamId":stream,"offset":offset,"base64":STANDARD.encode(chunk)}),
                )
                .unwrap();
            offset += chunk.len();
        }
        let committed = plugin_host::global()
            .invoke(id, "fs.commit", json!({"streamId":stream}))
            .unwrap();
        assert_ne!(committed["revision"], baseline["revision"]);
        assert!(plugin_host::global()
            .invoke(
                id,
                "fs.beginWrite",
                json!({"connection":config,"nodeId":"hello.txt","size":1,"ifRevision":baseline["revision"]}),
            )
            .is_err());
        let opened = plugin_host::global()
            .invoke(
                id,
                "fs.openRead",
                json!({"connection":config,"nodeId":"hello.txt"}),
            )
            .unwrap();
        let stream = opened["streamId"].as_str().unwrap();
        let readback = plugin_host::global()
            .invoke(
                id,
                "fs.readStream",
                json!({"streamId":stream,"offset":0,"maxBytes":1048576}),
            )
            .unwrap();
        assert_eq!(
            STANDARD
                .decode(readback["base64"].as_str().unwrap())
                .unwrap(),
            contents
        );
        plugin_host::global()
            .invoke(id, "fs.closeRead", json!({"streamId": stream}))
            .unwrap();
    }
    if id == "org.fiex.webdav" {
        let connection = fiex_lib::remote::save_connection(fiex_lib::remote::ConnectionInput {
            id: None,
            name: "Broker fixture".into(),
            kind: "webdav".into(),
            plugin_id: Some(id.into()),
            settings: config.as_object().unwrap().clone(),
        })
        .expect("Core connection authorization failed");
        let listing = futures::executor::block_on(fiex_lib::remote::list_listing(
            &connection.mount_path,
            false,
            &[],
        ))
        .unwrap()
        .unwrap();
        assert!(listing
            .entries
            .iter()
            .any(|entry| entry.name == "hello.txt"));
        let hello = format!("{}hello.txt", connection.mount_path);
        futures::executor::block_on(fiex_lib::remote::sync_one(&hello, &Default::default()))
            .unwrap();
        // Mount paths are opaque `fiex://` navigation URIs; downloaded bytes
        // live in Core's cache and are resolved through the public API.
        let cached = fiex_lib::remote::cached_path(&hello).unwrap();
        assert_eq!(std::fs::read(cached).unwrap(), expected);
        fiex_lib::remote::remove_connection(&connection.id).unwrap();
    }
    plugin_host::set_network_domain(id, "127.0.0.1", false).unwrap();
    assert!(plugin_host::global()
        .invoke(id, "connection.validate", json!({"connection":config}))
        .is_err());
}

fn main() {
    fiex_lib::plugins::security::sandbox::run_wrapper_if_requested();
    check(
        "org.fiex.webdav",
        json!({"url": "http://127.0.0.1:8080/"}),
        b"hello webdav\n",
    );
    check(
        "org.fiex.smb",
        json!({"host": "127.0.0.1", "port": 445, "share": "fiex", "username": "fiex", "password": std::env::var("FIEX_TEST_SMB_PASSWORD").unwrap()}),
        b"hello smb\n",
    );
    println!("PASS: sandboxed WebDAV/SMB SDK calls through Core and host grant revocation");
}
