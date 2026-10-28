use super::*;
fn reference(node: &str) -> Value {
    json!({"mountId":"local","nodeId":node})
}
fn operand(node: &str) -> String {
    sdk::ResourceRef {
        mount_id: "local".into(),
        node_id: node.into(),
    }
    .command_arg()
}
fn resource(node: &str) -> Value {
    reference(node)
}
#[test]
fn filenames_are_operands_and_output_is_not_overwritten() {
    let a = args(
        "gpg.decrypt",
        &json!({"input":resource("--options"),"output":resource("plain")}),
    )
    .unwrap();
    assert_eq!(a[a.len() - 2], "--");
    assert_eq!(a.last().unwrap(), &operand("--options"));
    assert!(!a.contains(&"--yes".to_string()));
    assert!(args(
        "gpg.decrypt",
        &json!({"input":resource("same"),"output":resource("same")})
    )
    .is_err());
}
#[test]
fn rejects_arbitrary_commands_and_key_options() {
    assert!(args("gpg.delete", &json!({})).is_err());
    assert!(args(
        "gpg.encrypt",
        &json!({"input":resource("x"),"output":resource("y"),"fingerprint":"--foo"})
    )
    .is_err());
}
#[test]
fn armor_and_trust_are_explicit_flags() {
    let plain = args(
        "gpg.encrypt",
        &json!({"input":resource("a"),"output":resource("b"),"fingerprint":"A".repeat(40)}),
    )
    .unwrap();
    assert!(!plain.contains(&"--armor".to_string()));
    assert!(!plain.contains(&"--trust-model".to_string()));
    let full = args(
        "gpg.encrypt",
        &json!({"input":resource("a"),"output":resource("b"),"fingerprint":"A".repeat(40),"armor":true,"trustUnverified":true}),
    )
    .unwrap();
    assert!(full.contains(&"--armor".to_string()));
    assert!(full.windows(2).any(|w| w == ["--trust-model", "always"]));
}
#[test]
fn encrypt_arguments_are_complete_and_ordered() {
    let fpr = "A".repeat(40);
    let args = args(
        "gpg.encrypt",
        &json!({"input":resource("in.txt"),"output":resource("out.gpg"),"fingerprint":fpr}),
    )
    .unwrap();
    assert!(args.contains(&"--batch".to_string()));
    assert!(args.contains(&"--no-tty".to_string()));
    assert!(args.contains(&"--status-fd=2".to_string()));
    assert!(args
        .windows(2)
        .any(|w| w == ["--output", operand("out.gpg").as_str()]));
    assert!(args.windows(2).any(|w| w == ["--recipient", fpr.as_str()]));
    let encrypt = args.iter().position(|a| a == "--encrypt").unwrap();
    let separator = args.iter().position(|a| a == "--").unwrap();
    assert!(
        encrypt < separator,
        "options must precede the operand separator"
    );
    assert_eq!(&args[separator + 1..], &[operand("in.txt")]);
}

#[test]
fn decrypt_never_requests_armor_even_if_asked() {
    let args = args(
        "gpg.decrypt",
        &json!({"input":resource("in.gpg"),"output":resource("out"),"armor":true}),
    )
    .unwrap();
    assert!(!args.contains(&"--armor".to_string()));
    assert!(args
        .windows(2)
        .any(|w| w == ["--output", operand("out").as_str()]));
    assert_eq!(args[args.len() - 2], "--");
    assert_eq!(args.last().unwrap(), &operand("in.gpg"));
}

#[test]
fn signing_uses_the_selected_identity_and_optional_armor() {
    let fpr = "B".repeat(40);
    let plain = args(
        "gpg.sign",
        &json!({"input":resource("a"),"output":resource("a.sig"),"fingerprint":fpr}),
    )
    .unwrap();
    assert!(plain
        .windows(2)
        .any(|w| w == ["--local-user", fpr.as_str()]));
    assert!(plain.contains(&"--detach-sign".to_string()));
    assert!(!plain.contains(&"--armor".to_string()));
    let armored = args(
        "gpg.sign",
        &json!({"input":resource("a"),"output":resource("a.sig.asc"),"fingerprint":fpr,"armor":true}),
    )
    .unwrap();
    assert!(armored.contains(&"--armor".to_string()));
}

#[test]
fn export_and_import_use_the_operand_separator() {
    let fpr = "C".repeat(40);
    let export = args(
        "gpg.export",
        &json!({"output":resource("pub.asc"),"fingerprint":fpr,"armor":true}),
    )
    .unwrap();
    let separator = export.iter().position(|a| a == "--").unwrap();
    assert_eq!(&export[separator + 1..], &[fpr.as_str()]);
    let import = args("gpg.import", &json!({"input":resource("pub.asc")})).unwrap();
    assert_eq!(import[import.len() - 2], "--");
    assert_eq!(import.last().unwrap(), &operand("pub.asc"));
    let verify = args(
        "gpg.verify",
        &json!({"signature":resource("a.sig"),"input":resource("a")}),
    )
    .unwrap();
    assert_eq!(verify[verify.len() - 3], "--");
    assert_eq!(
        &verify[verify.len() - 2..],
        &[operand("a.sig"), operand("a")]
    );
    let info = args("gpg.info", &json!({})).unwrap();
    assert!(info.contains(&"--version".to_string()));
}

#[test]
fn fingerprints_must_be_full_and_hexadecimal() {
    let not_hex = "Z".repeat(40);
    for bad in ["", "ABCDEF", "--recipient", not_hex.as_str()] {
        assert!(
            args(
                "gpg.encrypt",
                &json!({"input":resource("a"),"output":resource("b"),"fingerprint":bad})
            )
            .is_err(),
            "fingerprint must be rejected: {bad:?}"
        );
    }
    let hex = "0123456789abcdef".repeat(4);
    for len in [40usize, 64] {
        let fpr = &hex[..len];
        assert!(args(
            "gpg.encrypt",
            &json!({"input":resource("a"),"output":resource("b"),"fingerprint":fpr})
        )
        .is_ok());
    }
}

#[test]
fn create_key_uses_quick_generate_and_stdin_passphrase() {
    let args = args(
        "gpg.createKey",
        &json!({"name":"Ada Lovelace","email":"ada@example.org","expiry":"2y"}),
    )
    .unwrap();
    assert!(args.contains(&"--batch".to_string()));
    assert!(args
        .windows(2)
        .any(|w| w == ["--pinentry-mode", "loopback"]));
    assert!(args.windows(2).any(|w| w == ["--passphrase-fd", "0"]));
    assert!(args
        .windows(2)
        .any(|w| w == ["--quick-generate-key", "Ada Lovelace <ada@example.org>"]));
    assert_eq!(&args[args.len() - 3..], &["default", "default", "2y"]);
}

#[test]
fn create_key_rejects_option_injection_and_bad_expiry() {
    for value in [
        json!({"name":"-x","email":"a@b"}),
        json!({"name":"A","email":"not-an-email"}),
        json!({"name":"A","email":"a@b","expiry":"forever"}),
        json!({"name":"A <x>","email":"a@b"}),
    ] {
        assert!(args("gpg.createKey", &value).is_err(), "{value}");
    }
}

#[test]
fn delete_requires_a_full_fingerprint_and_user_confirmation() {
    let fpr = "E".repeat(40);
    let public = args("gpg.deleteKey", &json!({"fingerprint":fpr})).unwrap();
    assert!(public.contains(&"--yes".to_string()));
    assert!(public.contains(&"--delete-keys".to_string()));
    assert!(public.windows(2).any(|w| w == ["--", fpr.as_str()]));
    let secret = args("gpg.deleteKey", &json!({"fingerprint":fpr,"secret":true})).unwrap();
    assert!(secret.contains(&"--delete-secret-and-public-keys".to_string()));
    assert!(args("gpg.deleteKey", &json!({"fingerprint":"ABCDEF"})).is_err());
}

#[test]
fn export_can_select_secret_keys() {
    let fpr = "D".repeat(40);
    let public = args(
        "gpg.export",
        &json!({"output":resource("pub.asc"),"fingerprint":fpr}),
    )
    .unwrap();
    assert!(public.contains(&"--export".to_string()));
    assert!(!public.contains(&"--export-secret-keys".to_string()));
    let secret = args(
        "gpg.export",
        &json!({"output":resource("sec.asc"),"fingerprint":fpr,"secret":true}),
    )
    .unwrap();
    assert!(secret.contains(&"--export-secret-keys".to_string()));
    let listing = args("gpg.secretKeys", &json!({})).unwrap();
    assert!(listing.contains(&"--list-secret-keys".to_string()));
}
