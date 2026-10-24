//! Read-only archive listings for the preview panel.
use fiex_plugin_sdk as sdk;
use serde_json::{json, Value};
use std::io::{Read, Seek};

const MAX_ENTRIES: usize = 5000;

fn entries_from_zip<R: Read + Seek>(reader: R) -> Result<(Vec<Value>, bool), String> {
    let mut archive = zip::ZipArchive::new(reader).map_err(|e| e.to_string())?;
    let mut entries = Vec::new();
    let mut encrypted = false;
    for index in 0..archive.len().min(MAX_ENTRIES) {
        let file = archive.by_index_raw(index).map_err(|e| e.to_string())?;
        encrypted |= file.encrypted();
        entries.push(json!({
            "name": file.name(),
            "size": file.size(),
            "isDir": file.is_dir(),
            "encrypted": file.encrypted(),
        }));
    }
    Ok((entries, encrypted))
}

fn entries_from_tar<R: Read>(reader: R) -> Result<Vec<Value>, String> {
    let mut archive = tar::Archive::new(reader);
    let mut entries = Vec::new();
    for entry in archive.entries().map_err(|e| e.to_string())? {
        if entries.len() >= MAX_ENTRIES {
            break;
        }
        let entry = entry.map_err(|e| e.to_string())?;
        entries.push(json!({
            "name": entry.path().map(|path| path.to_string_lossy().into_owned()).unwrap_or_default(),
            "size": entry.size(),
            "isDir": entry.header().entry_type().is_dir(),
            "encrypted": false,
        }));
    }
    Ok(entries)
}

pub fn looks_encrypted(message: &str) -> bool {
    let lower = message.to_lowercase();
    lower.contains("password") || lower.contains("encrypt")
}

pub fn list(
    source: &str,
    password: Option<&str>,
    host: &sdk::RequestContext,
) -> Result<Value, String> {
    let lower = source.to_lowercase();
    let password = password.filter(|value| !value.is_empty());
    let files = host.files();
    if lower.ends_with(".zip") {
        let (entries, encrypted) =
            entries_from_zip(files.open_read(source).map_err(|error| error.to_string())?)?;
        Ok(json!({ "entries": entries, "encrypted": encrypted, "supported": true }))
    } else if lower.ends_with(".7z") {
        let mut reader = sevenz_rust2::ArchiveReader::new(
            files.open_read(source).map_err(|error| error.to_string())?,
            password.unwrap_or("").into(),
        )
        .map_err(|error| error.to_string())?;
        let mut entries = Vec::new();
        let mut failure = None;
        reader
            .for_each_entries(|entry, _reader| {
                if entries.len() >= MAX_ENTRIES {
                    return Ok(false);
                }
                entries.push(json!({
                    "name": entry.name,
                    "size": entry.size,
                    "isDir": entry.is_directory,
                    "encrypted": entry.has_stream && entry.size == 0,
                }));
                Ok(true)
            })
            .map_err(|error| error.to_string())?;
        if let Some(error) = failure.take() {
            return Err(error);
        }
        Ok(json!({ "entries": entries, "encrypted": false, "supported": true }))
    } else if lower.ends_with(".tar") {
        Ok(json!({
            "entries": entries_from_tar(files.open_read(source).map_err(|error| error.to_string())?)?,
            "encrypted": false,
            "supported": true
        }))
    } else if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        let reader = flate2::read::GzDecoder::new(
            files.open_read(source).map_err(|error| error.to_string())?,
        );
        Ok(json!({ "entries": entries_from_tar(reader)?, "encrypted": false, "supported": true }))
    } else if lower.ends_with(".tar.bz2") || lower.ends_with(".tbz2") {
        let reader = bzip2::read::BzDecoder::new(
            files.open_read(source).map_err(|error| error.to_string())?,
        );
        Ok(json!({ "entries": entries_from_tar(reader)?, "encrypted": false, "supported": true }))
    } else if lower.ends_with(".tar.xz") || lower.ends_with(".txz") {
        let reader =
            xz2::read::XzDecoder::new(files.open_read(source).map_err(|error| error.to_string())?);
        Ok(json!({ "entries": entries_from_tar(reader)?, "encrypted": false, "supported": true }))
    } else if lower.ends_with(".rar") {
        Ok(json!({
            "entries": [],
            "encrypted": false,
            "supported": false,
            "message": "RAR 暂不支持列目录，可直接解压",
        }))
    } else {
        Err("暂不支持预览此格式".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn zip_bytes() -> Vec<u8> {
        let mut buffer = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut buffer);
            writer
                .start_file("a.txt", zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(b"hello").unwrap();
            writer
                .add_directory("docs/", zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.finish().unwrap();
        }
        buffer.into_inner()
    }

    #[test]
    fn lists_zip_entries_and_directories() {
        let (entries, encrypted) = entries_from_zip(std::io::Cursor::new(zip_bytes())).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0]["name"], "a.txt");
        assert_eq!(entries[1]["isDir"], true);
        assert!(!encrypted);
    }

    #[test]
    fn detects_password_errors() {
        assert!(looks_encrypted("Password required"));
        assert!(looks_encrypted("archive is encrypted"));
        assert!(!looks_encrypted("no such file"));
    }
}
