//! Cross-repository smoke test: real Git plugin through Core's process host
//! and generic plugin VFS router. Not shipped in release bundles.
use fiex_lib::{plugin_decorations, plugin_host, plugin_vfs};
use serde_json::json;

fn main() {
    fiex_lib::plugin_sandbox::run_wrapper_if_requested();
    let id = "org.fiex.git";
    let entry = plugin_host::catalog()
        .into_iter()
        .find(|plugin| plugin.id == id)
        .expect("Git plugin must be installed in FIEX_PLUGIN_DIR");
    assert!(entry.backend_available);
    let (manifest, _) = plugin_host::installed_manifests().into_iter().find(|(m,_)|m.id==id).unwrap();
    plugin_host::activate(id, &plugin_host::activation_requirements(&manifest).unwrap()).unwrap();

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
    plugin_host::set_active_directory(&root).expect("active directory");

    let init = plugin_host::global()
        .invoke(id, "git.init", json!({"root": root}))
        .expect("git.init failed");
    assert_eq!(init["branch"], "main", "{init}");
    plugin_host::global()
        .invoke(
            id,
            "git.stage",
            json!({"root": root, "paths": [], "all": true}),
        )
        .expect("git.stage failed");
    let commit = plugin_host::global()
        .invoke(
            id,
            "git.commit",
            json!({"root": root, "message": "initial", "name": "Fiex", "email": "fiex@example.com"}),
        )
        .expect("git.commit failed");
    let hash = commit.as_str().expect("commit hash").to_string();
    assert_eq!(hash.len(), 7, "{hash}");

    let history = plugin_host::global()
        .invoke(id, "git.history", json!({"root": root, "skip": 0}))
        .expect("git.history failed");
    assert_eq!(history.as_array().unwrap().len(), 1, "{history}");

    let reference = plugin_host::global()
        .invoke(id, "git.browse", json!({"root": root, "reference": "main"}))
        .expect("git.browse failed");
    let history_path = reference.as_str().expect("history path").to_string();
    assert!(history_path.starts_with("history://"), "{history_path}");

    let listing = plugin_vfs::list(&history_path, true, &[]).expect("vfs.list failed");
    assert_eq!(listing.entries.len(), 1, "{:?}", listing.entries);
    assert_eq!(listing.entries[0].name, "a.txt");
    let child = listing.entries[0].path.clone();
    let info = plugin_vfs::stat(&child).expect("vfs.stat failed");
    assert_eq!(info.name, "a.txt");
    let preview = plugin_vfs::read_small(&child, 1024).expect("vfs.readSmall failed");
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
        }],
    );
    assert_eq!(decorated.len(), 1, "{decorated:?}");
    assert_eq!(decorated[0].badges[0].text, "M", "{decorated:?}");

    plugin_host::global().stop(id);
    std::fs::remove_dir_all(&repo).ok();
    println!("PASS: Git plugin commands and generic plugin VFS");
}
