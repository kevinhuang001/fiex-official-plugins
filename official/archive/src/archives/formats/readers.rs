//! Per-format archive readers and writers.
use super::super::{
    err,
    extract::extract_file,
    paths::{safe_name, text},
    MAX_ENTRIES, MAX_TOTAL,
};
use crate::ignores::IgnoreSet;
use fiex_plugin_sdk::{files::FileSystem, worker::HostProxy};
use std::{
    io::{self, Read, Write},
    path::Path,
};

pub(crate) fn extract_zip(
    files: &FileSystem<'_>,
    root: &Path,
    source: &str,
    password: Option<&str>,
    ignore: &IgnoreSet,
    remaining: &mut u64,
) -> Result<(), String> {
    let mut archive = zip::ZipArchive::new(files.open_read(source).map_err(err)?).map_err(err)?;
    if archive.len() > MAX_ENTRIES {
        return Err("压缩包项目过多".into());
    }
    for i in 0..archive.len() {
        let mut input = if let Some(password) = password {
            archive
                .by_index_decrypt(i, password.as_bytes())
                .map_err(err)?
        } else {
            archive.by_index(i).map_err(err)?
        };
        let name = input
            .enclosed_name()
            .ok_or("压缩包包含越界路径")?
            .to_owned();
        if input.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
            return Err("不解压符号链接".into());
        }
        if ignore.is_ignored(&name) {
            continue;
        }
        let dir = input.is_dir();
        extract_file(files, root, &name, dir, &mut input, remaining)?;
    }
    Ok(())
}

pub(crate) fn extract_7z(
    files: &FileSystem<'_>,
    root: &Path,
    source: &str,
    password: Option<&str>,
    ignore: &IgnoreSet,
    remaining: &mut u64,
) -> Result<(), String> {
    let mut archive = sevenz_rust2::ArchiveReader::new(
        files.open_read(source).map_err(err)?,
        password.unwrap_or("").into(),
    )
    .map_err(err)?;
    let mut count = 0;
    archive
        .for_each_entries(|meta, input| {
            count += 1;
            if count > MAX_ENTRIES {
                return Err(sevenz_rust2::Error::from(io::Error::other(
                    "压缩包项目过多",
                )));
            }
            if ignore.is_ignored(Path::new(&meta.name)) {
                return Ok(true);
            }
            extract_file(
                files,
                root,
                Path::new(&meta.name),
                meta.is_directory,
                input,
                remaining,
            )
            .map_err(|e| sevenz_rust2::Error::from(io::Error::other(e)))?;
            Ok(true)
        })
        .map_err(err)?;
    Ok(())
}

pub(crate) fn extract_rar(
    files: &FileSystem<'_>,
    root: &Path,
    source: &str,
    password: Option<&str>,
    ignore: &IgnoreSet,
    proxy: Option<HostProxy>,
) -> Result<(), String> {
    use std::sync::{Arc, Mutex};
    struct Sink(Arc<Mutex<Option<fiex_plugin_sdk::files::OwnedFileWriter>>>);
    impl Write for Sink {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0
                .lock()
                .map_err(|_| io::Error::other("write lock poisoned"))?
                .as_mut()
                .ok_or_else(|| io::Error::other("writer closed"))?
                .write(bytes)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let proxy = proxy.ok_or("RAR requires a host worker session")?;
    let reader =
        fiex_plugin_sdk::files::OwnedFileReader::open(proxy.clone(), source).map_err(err)?;
    let mut pending: Option<Arc<Mutex<Option<fiex_plugin_sdk::files::OwnedFileWriter>>>> = None;
    let mut count = 0;
    let mut options = rars::ArchiveReadOptions::default();
    options.password = password.map(str::as_bytes);
    options.max_total_output_bytes = Some(MAX_TOTAL);
    options.max_reader_workspace_bytes = Some(256 * 1024 * 1024);
    options.max_header_count = Some(MAX_ENTRIES as u64 + 1024);
    options.max_header_bytes = Some(64 * 1024 * 1024);
    let archive = rars::ArchiveReader::read_reader_with_options(reader, options).map_err(err)?;
    archive
        .extract_to_with_options(options, |meta| {
            if let Some(previous) = pending.take() {
                previous
                    .lock()
                    .map_err(|_| rars::Error::from(io::Error::other("writer lock poisoned")))?
                    .take()
                    .ok_or_else(|| rars::Error::from(io::Error::other("writer closed")))?
                    .commit()
                    .map_err(rars::Error::from)?;
            }
            count += 1;
            if count > MAX_ENTRIES {
                return Err(rars::Error::from(io::Error::other("压缩包项目过多")));
            }
            let name = String::from_utf8(meta.name.clone())
                .map_err(|e| rars::Error::from(io::Error::other(e)))?;
            safe_name(Path::new(&name)).map_err(|e| rars::Error::from(io::Error::other(e)))?;
            if ignore.is_ignored(Path::new(&name)) {
                return Ok(Box::new(io::sink()));
            }
            let path = root.join(name);
            if meta.is_directory {
                files
                    .create_dir_all(&text(&path))
                    .map_err(rars::Error::from)?;
                return Ok(Box::new(io::sink()));
            }
            if let Some(parent) = path.parent() {
                files
                    .create_dir_all(&text(parent))
                    .map_err(rars::Error::from)?;
            }
            let output = Arc::new(Mutex::new(Some(
                fiex_plugin_sdk::files::OwnedFileWriter::create(proxy.clone(), &text(&path))
                    .map_err(rars::Error::from)?,
            )));
            pending = Some(output.clone());
            Ok(Box::new(Sink(output)))
        })
        .map_err(err)?;
    if let Some(output) = pending {
        output
            .lock()
            .map_err(|_| "writer lock poisoned")?
            .take()
            .ok_or("writer closed")?
            .commit()
            .map_err(err)?;
    }
    Ok(())
}

pub(crate) fn extract_stream(
    files: &FileSystem<'_>,
    root: &Path,
    source: &str,
    source_path: &Path,
    lower: &str,
    ignore: &IgnoreSet,
    remaining: &mut u64,
) -> Result<(), String> {
    let reader = files.open_read(source).map_err(err)?;
    let reader: Box<dyn Read + '_> = if lower.ends_with(".gz") || lower.ends_with(".tgz") {
        Box::new(flate2::read::GzDecoder::new(reader))
    } else if lower.ends_with(".bz2") || lower.ends_with(".tbz2") {
        Box::new(bzip2::read::BzDecoder::new(reader))
    } else if lower.ends_with(".xz") || lower.ends_with(".txz") {
        Box::new(xz2::read::XzDecoder::new(reader))
    } else if lower.ends_with(".tar") {
        Box::new(reader)
    } else {
        return Err("不支持的压缩格式".into());
    };
    let tar = lower.ends_with(".tar")
        || [".tar.gz", ".tar.bz2", ".tar.xz", ".tgz", ".tbz2", ".txz"]
            .iter()
            .any(|s| lower.ends_with(s));
    if tar {
        let mut archive = tar::Archive::new(reader);
        for (i, input) in archive.entries().map_err(err)?.enumerate() {
            if i >= MAX_ENTRIES {
                return Err("压缩包项目过多".into());
            }
            let mut input = input.map_err(err)?;
            let name = input.path().map_err(err)?.into_owned();
            let kind = input.header().entry_type();
            if !kind.is_file() && !kind.is_dir() {
                return Err("不解压链接或特殊文件".into());
            }
            if ignore.is_ignored(&name) {
                continue;
            }
            extract_file(files, root, &name, kind.is_dir(), &mut input, remaining)?;
        }
    } else {
        extract_file(
            files,
            root,
            Path::new(source_path.file_stem().ok_or("无效文件名")?),
            false,
            &mut { reader },
            remaining,
        )?;
    }
    Ok(())
}
