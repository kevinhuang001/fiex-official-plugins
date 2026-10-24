//! Real HTTP and TCP SDK calls through Core, including explicit host consent.
use base64::{engine::general_purpose::STANDARD, Engine};
use fiex_lib::plugin_host;
use serde_json::{json, Value};

fn check(id: &str, config: Value, expected: &[u8]) {
    let (manifest, _) = plugin_host::installed_manifests()
        .into_iter()
        .find(|(manifest, _)| manifest.id == id)
        .expect("network plugin must be installed");
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
    let result = plugin_host::global()
        .invoke(
            id,
            "fs.readSmall",
            json!({"connection": config, "nodeId": "hello.txt", "maxBytes": 1024}),
        )
        .expect("network preview through Core failed");
    assert_eq!(
        STANDARD.decode(result["base64"].as_str().unwrap()).unwrap(),
        expected
    );
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
        let params = json!({"connection":config,"nodeId":"hello.txt","source":"source.dat","ifRevision":baseline["revision"]});
        // The exact same call without a selection cannot read the snapshot.
        assert!(plugin_host::global()
            .invoke(id, "fs.writeFile", params.clone())
            .is_err());
        let committed = plugin_host::global()
            .invoke_selected(
                id,
                "fs.writeFile",
                params.clone(),
                source_dir.path().to_str().unwrap(),
                std::slice::from_ref(&source),
            )
            .unwrap();
        assert_ne!(committed["revision"], baseline["revision"]);
        assert!(plugin_host::global()
            .invoke_selected(
                id,
                "fs.writeFile",
                params,
                source_dir.path().to_str().unwrap(),
                std::slice::from_ref(&source)
            )
            .is_err());
        let readback = plugin_host::global()
            .invoke(
                id,
                "fs.readSmall",
                json!({"connection":config,"nodeId":"hello.txt","maxBytes":1048576}),
            )
            .unwrap();
        assert_eq!(
            STANDARD
                .decode(readback["base64"].as_str().unwrap())
                .unwrap(),
            contents
        );
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
        let hello = format!("{}/hello.txt", connection.mount_path);
        futures::executor::block_on(fiex_lib::remote::sync_one(&hello, &Default::default()))
            .unwrap();
        assert_eq!(std::fs::read(&hello).unwrap(), expected);
        fiex_lib::remote::remove_connection(&connection.id).unwrap();
    }
    plugin_host::set_network_domain(id, "127.0.0.1", false).unwrap();
    assert!(plugin_host::global()
        .invoke(id, "plugin.ping", json!({}))
        .is_err());
}

fn main() {
    fiex_lib::plugin_sandbox::run_wrapper_if_requested();
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
