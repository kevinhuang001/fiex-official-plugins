//! Cross-repository test for dynamic-only decorations with manifest match rules.
use fiex_lib::plugins::host as plugin_host;
use fiex_lib::plugins::resolvers::decorations::DecorationEntry;

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
    let (manifest, _) = plugin_host::installed_manifests()
        .into_iter()
        .find(|(manifest, _)| manifest.id == "org.fiex.archive")
        .expect("archive plugin must be installed");
    ensure_approved("org.fiex.archive");
    plugin_host::activate(
        "org.fiex.archive",
        &plugin_host::activation_requirements(&manifest).unwrap(),
    )
    .unwrap();
    let root = std::env::temp_dir();
    let path = root.join("decoration-fixture.zip");
    let values = fiex_lib::plugins::resolvers::decorations::query(
        root.to_str().unwrap(),
        vec![DecorationEntry {
            path: path.to_string_lossy().into_owned(),
            name: "decoration-fixture.zip".into(),
            is_dir: false,
            extension: "zip".into(),
            mime: "application/zip".into(),
            ..Default::default()
        }],
    );
    let decoration = values
        .iter()
        .find(|value| value.path == path.to_string_lossy())
        .expect("archive dynamic decoration missing");
    let has_zip = decoration
        .badges
        .iter()
        .chain(
            decoration
                .fields
                .get("name")
                .map(|field| field.badges.iter())
                .into_iter()
                .flatten(),
        )
        .any(|badge| badge.text == "ZIP");
    assert!(has_zip, "{decoration:?}");
    plugin_host::global().stop("org.fiex.archive");
    println!("PASS: dynamic decorations and manifest match rules");
}
