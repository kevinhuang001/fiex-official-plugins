use md5::{Digest as Md5Digest, Md5};
use serde::Serialize;
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};
use std::io::Read;

#[derive(Serialize)]
pub struct Checksum {
    pub algorithm: &'static str,
    pub value: String,
}

/// Read once, with bounded memory, regardless of file size or algorithm count.
pub fn calculate(mut file: impl Read) -> Result<Vec<Checksum>, String> {
    let mut md5 = Md5::new();
    let mut sha1 = Sha1::new();
    let mut sha256 = Sha256::new();
    let mut sha512 = Sha512::new();
    let mut crc = crc32fast::Hasher::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        let bytes = &buffer[..count];
        md5.update(bytes);
        sha1.update(bytes);
        sha256.update(bytes);
        sha512.update(bytes);
        crc.update(bytes);
    }
    Ok(vec![
        Checksum {
            algorithm: "SHA-256",
            value: format!("{:x}", sha256.finalize()),
        },
        Checksum {
            algorithm: "SHA-512",
            value: format!("{:x}", sha512.finalize()),
        },
        Checksum {
            algorithm: "SHA-1",
            value: format!("{:x}", sha1.finalize()),
        },
        Checksum {
            algorithm: "MD5",
            // md-5 0.11 returns a hybrid_array digest that does not implement
            // LowerHex, so render the bytes explicitly.
            value: md5
                .finalize()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        },
        Checksum {
            algorithm: "CRC32",
            value: format!("{:08x}", crc.finalize()),
        },
    ])
}

/// Compute only the requested digest algorithms while reading the file once.
pub fn calculate_selected(
    mut file: impl Read,
    requested: &[&str],
) -> Result<Vec<Checksum>, String> {
    let requested: std::collections::BTreeSet<_> = requested.iter().copied().collect();
    let mut md5 = requested.contains("md5").then(Md5::new);
    let mut sha1 = requested.contains("sha1").then(Sha1::new);
    let mut sha256 = requested.contains("sha256").then(Sha256::new);
    let mut sha512 = requested.contains("sha512").then(Sha512::new);
    let mut crc = requested.contains("crc32").then(crc32fast::Hasher::new);
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        let bytes = &buffer[..count];
        if let Some(value) = &mut md5 {
            value.update(bytes);
        }
        if let Some(value) = &mut sha1 {
            value.update(bytes);
        }
        if let Some(value) = &mut sha256 {
            value.update(bytes);
        }
        if let Some(value) = &mut sha512 {
            value.update(bytes);
        }
        if let Some(value) = &mut crc {
            value.update(bytes);
        }
    }
    let mut result = Vec::with_capacity(requested.len());
    for algorithm in requested {
        let hash = match algorithm {
            "md5" => md5.take().map(|value| Checksum {
                algorithm: "MD5",
                value: value
                    .finalize()
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect(),
            }),
            "sha1" => sha1.take().map(|value| Checksum {
                algorithm: "SHA-1",
                value: format!("{:x}", value.finalize()),
            }),
            "sha256" => sha256.take().map(|value| Checksum {
                algorithm: "SHA-256",
                value: format!("{:x}", value.finalize()),
            }),
            "sha512" => sha512.take().map(|value| Checksum {
                algorithm: "SHA-512",
                value: format!("{:x}", value.finalize()),
            }),
            "crc32" => crc.take().map(|value| Checksum {
                algorithm: "CRC32",
                value: format!("{:08x}", value.finalize()),
            }),
            _ => return Err(format!("unsupported checksum algorithm: {algorithm}")),
        };
        if let Some(hash) = hash {
            result.push(hash);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn known_vectors() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), b"abc").unwrap();
        let hashes = calculate(std::fs::File::open(file.path()).unwrap()).unwrap();
        assert_eq!(
            hashes[0].value,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(hashes[2].value, "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(hashes[3].value, "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(hashes[4].value, "352441c2");
    }

    #[test]
    fn selected_hash_computes_only_the_requested_algorithm() {
        let hashes = calculate_selected(&b"abc"[..], &["sha256"]).unwrap();
        assert_eq!(hashes.len(), 1);
        assert_eq!(hashes[0].algorithm, "SHA-256");
        assert_eq!(
            hashes[0].value,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
