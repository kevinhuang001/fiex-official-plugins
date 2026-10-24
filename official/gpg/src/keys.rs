//! Public key listing: parses the machine-readable `--with-colons` records.
use serde_json::{json, Value};

pub(crate) fn keys(stdout: &str) -> Vec<Value> {
    let mut keys: Vec<Value> = Vec::new();
    let mut current: Option<(String, String, String, String)> = None;
    for line in stdout.lines() {
        let fields: Vec<&str> = line.split(':').collect();
        let record = fields.first().copied();
        match record {
            Some("pub") => {
                current = Some((
                    String::new(),
                    fields.get(4).copied().unwrap_or("").to_string(),
                    String::new(),
                    fields.get(6).copied().unwrap_or("").to_string(),
                ));
            }
            Some("fpr") if fields.len() > 9 => {
                if let Some(entry) = current.as_mut() {
                    if entry.0.is_empty() {
                        entry.0 = fields[9].to_string();
                    }
                }
            }
            Some("uid") if fields.len() > 9 => {
                if let Some(entry) = current.as_mut() {
                    if entry.2.is_empty() {
                        entry.2 = fields[9].to_string();
                    }
                }
            }
            _ => {}
        }
        // A key is complete once its primary fingerprint and uid are known.
        // Later subkeys must not overwrite the primary identity.
        if record != Some("pub") {
            if let Some((fingerprint, key_id, user, expires)) = current.as_ref() {
                if !fingerprint.is_empty() && !user.is_empty() {
                    keys.push(json!({
                        "fingerprint": fingerprint,
                        "keyId": key_id,
                        "name": user,
                        "email": user
                            .rsplit_once('<')
                            .and_then(|(_, rest)| rest.split_once('>'))
                            .map_or(String::new(), |(email, _)| email.to_string()),
                        "expires": expires,
                    }));
                    current = None;
                }
            }
        }
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keys_skip_subkeys_and_orphans_and_keep_every_primary() {
        let list = "pub:u:4096:1:AAAA0000AAAA0000:1:2::u:::scESC:::::::\nfpr:::::::::1111111111111111111111111111111111111111:\nuid:u::::1::H::First <a@x>::::::::::0:\nsub:u:4096:1:BBBB1111BBBB1111:1::::::e::::::\nfpr:::::::::2222222222222222222222222222222222222222:\npub:u:2048:1:CCCC2222CCCC2222:1:2::u:::scESC:::::::\nfpr:::::::::3333333333333333333333333333333333333333:\nuid:u::::1::H::Second <b@x>::::::::::0:\n";
        let parsed = keys(list);
        assert_eq!(parsed.len(), 2, "{parsed:?}");
        assert_eq!(
            parsed[0]["fingerprint"],
            "1111111111111111111111111111111111111111"
        );
        assert_eq!(parsed[1]["email"], "b@x");
        assert!(parsed
            .iter()
            .all(|k| k["fingerprint"] != "2222222222222222222222222222222222222222"));
    }

    #[test]
    fn parses_primary_keys_and_their_identity() {
        let list = "pub:u:4096:1:ABCDEF0123456789:1700000000:1800000000::u:::scESC:::::::\nfpr:::::::::0123456789ABCDEF0123456789ABCDEF01234567:\nuid:u::::1700000000::HASH::Ada Lovelace <ada@example.org>::::::::::0:\nsub:u:4096:1:1111222233334444:1700000000::::::e::::::\nfpr:::::::::9999888877776666555544443333222211110000:\n";
        let parsed = keys(list);
        assert_eq!(parsed.len(), 1);
        assert_eq!(
            parsed[0]["fingerprint"],
            "0123456789ABCDEF0123456789ABCDEF01234567"
        );
        assert_eq!(parsed[0]["keyId"], "ABCDEF0123456789");
        assert_eq!(parsed[0]["name"], "Ada Lovelace <ada@example.org>");
        assert_eq!(parsed[0]["email"], "ada@example.org");
    }
}
