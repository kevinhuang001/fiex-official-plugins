//! Cross-repository smoke test: real Git plugin through Core's process host
//! and generic plugin VFS router. Not shipped in release bundles.
#[path = "../support/fixtures.rs"]
mod fixtures;
use fiex_lib::{
    plugins::host as plugin_host, plugins::resolvers::decorations as plugin_decorations,
    plugins::vfs as plugin_vfs,
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

fn main() {
    fiex_lib::plugins::security::sandbox::run_wrapper_if_requested();
    let id = "org.fiex.git";
    let entry = plugin_host::catalog()
        .into_iter()
        .find(|plugin| plugin.id == id)
        .expect("Git plugin must be installed in FIEX_PLUGIN_DIR");
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

    let repo = std::env::temp_dir().join(format!("fiex-git-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&repo);
    std::fs::create_dir_all(&repo).unwrap();
    std::process::Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(&repo)
        .status()
        .unwrap();
    std::fs::write(repo.join("a.txt"), "hello\n").unwrap();
    let root = repo.to_string_lossy().into_owned();
    let invoke = |method: &str, params: serde_json::Value| {
        fixtures::invoke_selected(
            id,
            method,
            params,
            &root,
            &[std::path::PathBuf::from(&root)],
        )
    };

    let init = invoke(
        "git.init",
        json!({"root": fixtures::reference(std::path::Path::new(&root))}),
    )
    .expect("git.init failed");
    assert_eq!(init["branch"], "main", "{init}");
    invoke(
        "git.stage",
        json!({"root": fixtures::reference(std::path::Path::new(&root)), "paths": [], "all": true}),
    )
    .expect("git.stage failed");
    let commit = invoke(
        "git.commit",
        json!({"root": fixtures::reference(std::path::Path::new(&root)), "message": "initial", "name": "Fiex", "email": "fiex@example.com"}),
    )
    .expect("git.commit failed");
    let hash = commit.as_str().expect("commit hash").to_string();
    assert_eq!(hash.len(), 7, "{hash}");

    let history = invoke(
        "git.history",
        json!({"root": fixtures::reference(std::path::Path::new(&root)), "skip": 0}),
    )
    .expect("git.history failed");
    assert_eq!(history.as_array().unwrap().len(), 1, "{history}");

    let reference = invoke(
        "git.browse",
        json!({"root": fixtures::reference(std::path::Path::new(&root)), "reference": "main"}),
    )
    .expect("git.browse failed");
    let history_path = reference.as_str().expect("history path").to_string();
    assert!(history_path.starts_with("fiex://mount/"), "{history_path}");

    let listing = plugin_vfs::list(&history_path, true, &[]).expect("vfs.list failed");
    assert_eq!(listing.entries.len(), 1, "{:?}", listing.entries);
    assert_eq!(listing.entries[0].name, "a.txt");
    let child = listing.entries[0].path.clone();
    let info = plugin_vfs::stat(&child).expect("vfs.stat failed");
    assert_eq!(info.name, "a.txt");
    let preview = plugin_vfs::read_prefix(&child, 1024).expect("fs.openRead failed");
    assert_eq!(String::from_utf8_lossy(&preview), "hello\n");
    let (found, _) =
        plugin_vfs::search(&history_path, "a.txt", true, &[], 50).expect("vfs.search failed");
    assert!(!found.is_empty(), "search returned no entries");

    std::fs::write(repo.join("a.txt"), "changed\n").unwrap();
    let decorated = plugin_decorations::query(
        &root,
        vec![plugin_decorations::DecorationEntry {
            path: repo.join("a.txt").to_string_lossy().into_owned(),
            name: "a.txt".into(),
            is_dir: false,
            extension: "txt".into(),
            mime: "text/plain".into(),
            ..Default::default()
        }],
    );
    assert_eq!(decorated.len(), 1, "{decorated:?}");
    // Badges live on the value cells since decorations can target columns; the
    // filename cell keeps the legacy top-level meaning.
    let name = decorated[0]
        .fields
        .get("name")
        .expect("filename decoration");
    assert_eq!(name.badges[0].text, "M", "{decorated:?}");

    // Two immutable contexts must continue to serve different commits.
    invoke(
        "git.stage",
        json!({"root":fixtures::reference(std::path::Path::new(&root)),"paths":[],"all":true}),
    )
    .unwrap();
    invoke(
        "git.commit",
        json!({"root":fixtures::reference(std::path::Path::new(&root)),"message":"second","name":"Fiex","email":"fiex@example.com"}),
    )
    .unwrap();
    let second = invoke(
        "git.browse",
        json!({"root":fixtures::reference(std::path::Path::new(&root)),"reference":"main"}),
    )
    .unwrap()
    .as_str()
    .unwrap()
    .to_string();
    assert_ne!(history_path, second);
    let second_file = plugin_vfs::list(&second, true, &[]).unwrap().entries[0]
        .path
        .clone();
    assert_eq!(
        plugin_vfs::read_prefix(&second_file, 1024).unwrap(),
        b"changed\n"
    );
    assert_eq!(plugin_vfs::read_prefix(&child, 1024).unwrap(), b"hello\n");
    assert!(fiex_lib::remote::connection_for_path(&child).is_none());
    assert!(plugin_vfs::list("fiex://mount/unknown/", true, &[]).is_err());
    assert!(plugin_host::global()
        .invoke(id, "fs.beginWrite", json!({"connection":{},"nodeId":"x"}))
        .is_err());
    let unselected = fiex_lib::plugins::resource::fs_access::Scope::selected(&repo, &[]).unwrap();
    assert!(fiex_lib::plugins::mounts::open_view(
        id,
        &json!({"contributionId":"history","resourceRoot":root,"viewContext":{}}),
        &unselected
    )
    .is_err());
    assert!(fiex_lib::plugins::mounts::open_view(
        id,
        &json!({"contributionId":"foreign","resourceRoot":root,"viewContext":{}}),
        &unselected
    )
    .is_err());
    plugin_host::set_enabled(id, false).unwrap();
    assert!(plugin_vfs::read_prefix(&child, 1024).is_err());
    assert!(plugin_vfs::handles(&child));
    plugin_host::global().stop(id);
    std::fs::remove_dir_all(&repo).ok();
    println!("PASS: Git plugin commands and generic plugin VFS");
}
