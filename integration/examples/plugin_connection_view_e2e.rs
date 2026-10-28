//! A packaged connection uses the generic Core navigation/PluginView bridge.
use fiex_lib::plugins::host;
use serde_json::{json, Value};

fn main() {
    fiex_lib::plugins::security::sandbox::run_wrapper_if_requested();
    let id = "org.fiex.ssh";
    let (manifest, _) = host::installed_manifests()
        .into_iter()
        .find(|(manifest, _)| manifest.id == id)
        .expect("install the SSH package before this test");
    if !host::catalog()
        .iter()
        .any(|plugin| plugin.id == id && plugin.approved)
    {
        host::approve_install(id).unwrap();
    }
    host::activate(id, &host::activation_requirements(&manifest).unwrap()).unwrap();
    let connection = fiex_lib::remote::save_connection(fiex_lib::remote::ConnectionInput {
        id: None,
        name: "PluginView integration".into(),
        kind: id.into(),
        plugin_id: Some(id.into()),
        settings: serde_json::from_value(json!({"host":"127.0.0.1","port":2222})).unwrap(),
    })
    .unwrap();
    let destination = fiex_lib::remote::open_navigation(&connection.id).unwrap();
    assert_eq!(destination["kind"], "pluginView");
    assert_eq!(destination["pluginId"], id);
    let state = &destination["state"];
    assert_eq!(state.as_object().unwrap().len(), 1);
    assert!(state["sessionId"].as_str().is_some());
    let html = std::fs::read_to_string(destination["assetPath"].as_str().unwrap()).unwrap();
    assert!(html.contains("ssh.start"));
    assert!(!html.contains("ssh_open"));
    assert!(!html.contains("host.terminal"));
    assert_eq!(
        host::global()
            .invoke(id, "ssh.close", state.clone())
            .unwrap(),
        Value::Null
    );
    fiex_lib::remote::remove_connection(&connection.id).unwrap();
    println!(
        "PASS: packaged SSH uses generic connection navigation, isolated PluginView and plugin RPC"
    );
}
