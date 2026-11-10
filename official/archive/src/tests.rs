use super::{archive_menu, hook_error_message};
use serde_json::{Value, json};

#[test]
fn compression_options_are_resolved_only_after_expanding() {
    let targets = json!({"targets":[{"metadata":{"name":"notes.txt","kind":"file","operations":{"read":true}}}]});
    let root = archive_menu(&targets);
    let items = root["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["kind"], "submenu");
    assert_eq!(items[0]["id"], "archive.compress");
    let children = archive_menu(&json!({
        "targets":targets["targets"],
        "parentId":"archive.compress"
    }));
    let items = children["items"].as_array().unwrap();
    assert!(items.len() >= 2);
    assert!(items.iter().all(|item| item.get("parentId").is_none()));
    assert!(items.iter().any(|item| item["id"] == "archive.zip"));
}

#[test]
fn menus_require_content_capabilities_without_source_predicates() {
    for metadata in [
        json!({"name":"system.zip","kind":"file","operations":{"stat":true,"read":false}}),
        json!({"name":"link.zip","kind":"symlink","operations":{"read":true}}),
        json!({"name":"opaque.zip","kind":"file"}),
    ] {
        assert!(
            archive_menu(&json!({"targets":[{"metadata":metadata}]}))["items"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
    let root = archive_menu(
        &json!({"targets":[{"metadata":{"name":"folder","kind":"directory","operations":{"list":true}}}]}),
    );
    assert_eq!(root["items"][0]["id"], "archive.compress");
}

#[test]
fn encrypted_hook_failures_point_at_the_password_actions() {
    assert!(hook_error_message("Password required").contains("密码"));
    assert!(hook_error_message("archive is encrypted").contains("预览"));
    assert_eq!(hook_error_message("磁盘已满"), "磁盘已满");
}

#[test]
fn manifest_negotiates_a_longer_ui_budget_for_archive_listing() {
    let manifest: Value = serde_json::from_str(include_str!("../manifest.json")).unwrap();
    let timeout = manifest["extensions"]["ui"]["timeoutMs"]
        .as_u64()
        .expect("manifest must declare extensions.ui.timeoutMs");
    assert!(
        timeout >= 120_000,
        "archive listing needs a longer budget than Core's old default"
    );
}

#[test]
fn extracting_archive_stays_a_single_action() {
    let root = archive_menu(
        &json!({"targets":[{"metadata":{"name":"sample.zip","kind":"file","operations":{"read":true}}}]}),
    );
    assert_eq!(root["items"][0]["id"], "archive.extract");
    assert_eq!(root["items"].as_array().unwrap().len(), 1);
    assert!(
        archive_menu(&json!({"targets":[]}))["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        archive_menu(&json!({"parentId":"foreign"}))["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
