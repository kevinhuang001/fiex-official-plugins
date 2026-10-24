//! Real GnuPG behind Core's authorized command broker, with an isolated keyring.
use fiex_lib::plugin_host;
use serde_json::json;
fn main() {
    fiex_lib::plugin_sandbox::run_wrapper_if_requested();
    let root = std::env::temp_dir().join(format!("fiex-gpg-e2e-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let home = root.join("keyring");
    std::fs::create_dir_all(&home).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    std::env::set_var("GNUPGHOME", &home);
    assert!(std::process::Command::new("gpg")
        .args([
            "--batch",
            "--pinentry-mode",
            "loopback",
            "--passphrase",
            "",
            "--quick-generate-key",
            "Fiex CI <fiex@example.invalid>",
            "rsa2048",
            "sign,encrypt",
            "1d"
        ])
        .status()
        .unwrap()
        .success());
    let (manifest, _) = plugin_host::installed_manifests()
        .into_iter()
        .find(|(m, _)| m.id == "org.fiex.gpg")
        .unwrap();
    plugin_host::activate(
        &manifest.id,
        &plugin_host::activation_requirements(&manifest).unwrap(),
    )
    .unwrap();
    let invoke = |method, params| {
        plugin_host::global().invoke_at("org.fiex.gpg", method, params, root.to_str().unwrap())
    };
    let keys = invoke("gpg.keys", json!({})).unwrap();
    let fingerprint = keys[0]["fingerprint"].as_str().unwrap();
    let input = root.join("plain.bin");
    let encrypted = root.join("encrypted.gpg");
    let output = root.join("decrypted.bin");
    let signature = root.join("signature.asc");
    let data = vec![0, 255, 128, 10, 42];
    std::fs::write(&input, &data).unwrap();
    invoke(
        "gpg.encrypt",
        json!({"input":input,"output":encrypted,"fingerprint":fingerprint}),
    )
    .unwrap();
    invoke("gpg.decrypt", json!({"input":encrypted,"output":output})).unwrap();
    assert_eq!(std::fs::read(&output).unwrap(), data);
    assert!(
        invoke("gpg.decrypt", json!({"input":encrypted,"output":output})).is_err(),
        "must not overwrite existing file"
    );
    invoke(
        "gpg.sign",
        json!({"input":input,"output":signature,"fingerprint":fingerprint}),
    )
    .unwrap();
    invoke("gpg.verify", json!({"input":input,"signature":signature})).unwrap();
    std::fs::write(&input, b"tampered").unwrap();
    assert!(invoke("gpg.verify", json!({"input":input,"signature":signature})).is_err());
    for id in ["org.fiex.theme.vista", "org.fiex.theme.win11"] {
        let (manifest, _) = plugin_host::installed_manifests()
            .into_iter()
            .find(|(m, _)| m.id == id)
            .unwrap();
        plugin_host::activate(
            id,
            &plugin_host::activation_requirements(&manifest).unwrap(),
        )
        .unwrap();
    }
    assert!(plugin_host::themes()
        .iter()
        .any(|theme| theme.id == "org.fiex.theme.vista:vista"));
    plugin_host::global().stop("org.fiex.gpg");
    let _ = std::process::Command::new("gpgconf")
        .args(["--kill", "gpg-agent"])
        .status();
    std::fs::remove_dir_all(root).unwrap();
}
