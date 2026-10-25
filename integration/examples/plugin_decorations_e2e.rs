//! Cross-repository test for dynamic-only decorations with manifest match rules.
use fiex_lib::plugin_decorations::DecorationEntry;
use fiex_lib::plugin_host;

fn main() {
    fiex_lib::plugin_sandbox::run_wrapper_if_requested();
    let (manifest, _) = plugin_host::installed_manifests()
        .into_iter()
        .find(|(manifest, _)| manifest.id == "org.fiex.archive")
        .expect("archive plugin must be installed");
    plugin_host::activate(
        "org.fiex.archive",
        &plugin_host::activation_requirements(&manifest).unwrap(),
    )
    .unwrap();
    let root = std::env::temp_dir();
    let path = root.join("decoration-fixture.zip");
    let values = fiex_lib::plugin_decorations::query(
        root.to_str().unwrap(),
        vec![DecorationEntry {
            path: path.to_string_lossy().into_owned(),
            name: "decoration-fixture.zip".into(),
            is_dir: false,
            extension: "zip".into(),
            mime: "application/zip".into(),
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
