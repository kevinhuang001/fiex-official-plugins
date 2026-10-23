//! Compression engines run in the plugin; every file operation uses the SDK.
use crate::ignores::IgnoreSet;
use fiex_plugin_sdk::{files::FileSystem, HostServices};
use std::{
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
};
const MAX_TOTAL: u64 = 20 * 1024 * 1024 * 1024;
const MAX_ENTRIES: usize = 100000;
fn err(error: impl ToString) -> String {
    error.to_string()
}
fn safe_name(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty()
        || path.to_string_lossy().contains(['\\', ':'])
        || path
            .components()
            .any(|p| !matches!(p, Component::Normal(_) | Component::CurDir))
    {
        Err("压缩包包含不安全路径".into())
    } else {
        Ok(())
    }
}
fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
fn unique(files: &FileSystem<'_>, path: &Path) -> Result<PathBuf, String> {
    for n in 0..1000 {
        let next = if n == 0 {
            path.to_path_buf()
        } else {
            path.with_file_name(format!(
                "{} ({})",
                path.file_name().unwrap_or_default().to_string_lossy(),
                n + 1
            ))
        };
        if files.metadata(&text(&next)).is_err() {
            return Ok(next);
        }
    }
    Err("无法生成不冲突的文件名".into())
}
struct Source {
    path: String,
    name: String,
    dir: bool,
    size: u64,
}
fn sources(
    files: &FileSystem<'_>,
    paths: &[String],
    ignore: &IgnoreSet,
) -> Result<Vec<Source>, String> {
    fn visit(
        files: &FileSystem<'_>,
        path: &str,
        name: &Path,
        ignore: &IgnoreSet,
        out: &mut Vec<Source>,
    ) -> Result<(), String> {
        if ignore.is_ignored(name) {
            return Ok(());
        }
        if out.len() >= MAX_ENTRIES {
            return Err("文件数量超过限制".into());
        }
        let meta = files.metadata(path).map_err(err)?;
        if meta.symlink {
            return Err("不跟随符号链接".into());
        }
        out.push(Source {
            path: path.into(),
            name: text(name).replace('\\', "/"),
            dir: meta.is_dir,
            size: meta.size,
        });
        if meta.is_dir {
            for child in files.list(path).map_err(err)? {
                visit(files, &child.path, &name.join(child.name), ignore, out)?;
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    for path in paths {
        let name = Path::new(path).file_name().ok_or("不能压缩根目录")?;
        visit(files, path, Path::new(name), ignore, &mut out)?;
    }
    Ok(out)
}
pub fn default_destination(
    paths: &[String],
    format: &str,
    host: &mut dyn HostServices,
) -> Result<String, String> {
    if !matches!(
        format,
        "zip" | "rar" | "7z" | "tar" | "tar.gz" | "tar.bz2" | "tar.xz"
    ) {
        return Err("不支持的压缩格式".into());
    }
    let first = Path::new(paths.first().ok_or("请选择文件")?);
    let parent = first.parent().ok_or("无效路径")?;
    let stem = if paths.len() == 1 {
        first
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    } else {
        "Archive".into()
    };

    let files = host.files();
    Ok(text(&unique(
        &files,
        &parent.join(format!("{stem}.{format}")),
    )?))
}
fn write_tar<W: Write>(files: &FileSystem<'_>, sources: &[Source], writer: W) -> Result<W, String> {
    let mut builder = tar::Builder::new(writer);
    for source in sources {
        let mut header = tar::Header::new_gnu();
        header.set_mode(if source.dir { 0o755 } else { 0o644 });
        header.set_size(if source.dir { 0 } else { source.size });
        header.set_entry_type(if source.dir {
            tar::EntryType::Directory
        } else {
            tar::EntryType::Regular
        });
        header.set_cksum();
        if source.dir {
            builder
                .append_data(&mut header, &source.name, io::empty())
                .map_err(err)?;
        } else {
            builder
                .append_data(
                    &mut header,
                    &source.name,
                    files.open_read(&source.path).map_err(err)?,
                )
                .map_err(err)?;
        }
    }
    builder.into_inner().map_err(err)
}
pub fn compress(
    paths: &[String],
    destination: &str,
    format: &str,
    ignores: &[String],
    level: Option<u32>,
    password: Option<&str>,
    host: &mut dyn HostServices,
) -> Result<String, String> {
    if paths.is_empty() {
        return Err("请选择文件".into());
    }
    let proxy = host.worker_proxy();
    let files = host.files();
    let sources = sources(&files, paths, &IgnoreSet::new(ignores))?;
    let writer = files.begin_write(destination, true).map_err(err)?;
    let level = level.unwrap_or(6).min(9);
    let password = password.filter(|p| !p.is_empty());
    if password.is_some() && !matches!(format, "zip" | "rar") {
        return Err("只有 ZIP 和 RAR 支持密码".into());
    }
    let writer = match format {
        "zip" => {
            let mut zip = zip::ZipWriter::new(writer);
            for source in &sources {
                let mut options = zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated)
                    .compression_level(Some(level as i64));
                if let Some(password) = password {
                    options = options.with_aes_encryption(zip::AesMode::Aes256, password);
                }
                if source.dir {
                    zip.add_directory(format!("{}/", source.name), options)
                        .map_err(err)?;
                } else {
                    zip.start_file(&source.name, options).map_err(err)?;
                    io::copy(&mut files.open_read(&source.path).map_err(err)?, &mut zip)
                        .map_err(err)?;
                }
            }
            zip.finish().map_err(err)?
        }
        "tar" => write_tar(&files, &sources, writer)?,
        "tar.gz" => write_tar(
            &files,
            &sources,
            flate2::write::GzEncoder::new(writer, flate2::Compression::new(level)),
        )?
        .finish()
        .map_err(err)?,
        "tar.bz2" => write_tar(
            &files,
            &sources,
            bzip2::write::BzEncoder::new(writer, bzip2::Compression::new(level.max(1))),
        )?
        .finish()
        .map_err(err)?,
        "tar.xz" => write_tar(&files, &sources, xz2::write::XzEncoder::new(writer, level))?
            .finish()
            .map_err(err)?,
        "7z" => {
            let mut archive = sevenz_rust2::ArchiveWriter::new(writer).map_err(err)?;
            for source in &sources {
                if source.dir {
                    archive
                        .push_archive_entry::<io::Empty>(
                            sevenz_rust2::ArchiveEntry::new_directory(&source.name),
                            None,
                        )
                        .map_err(err)?;
                } else {
                    archive
                        .push_archive_entry(
                            sevenz_rust2::ArchiveEntry::new_file(&source.name),
                            Some(files.open_read(&source.path).map_err(err)?),
                        )
                        .map_err(err)?;
                }
            }
            archive.finish().map_err(err)?
        }
        "rar" => {
            let mut builder = rars::Builder::new(rars::ArchiveVersion::Rar50)
                .compression_level(Some(level.min(5) as u8));
            if let Some(password) = password {
                builder = builder.password(Some(password.as_bytes().to_vec()));
            }
            let proxy = proxy.clone().ok_or("RAR requires a host worker session")?;
            for source in &sources {
                if source.dir {
                    builder
                        .add_directory(source.name.as_bytes().to_vec(), None, None)
                        .map_err(err)?;
                } else {
                    let proxy = proxy.clone();
                    let path = source.path.clone();
                    let input = rars::EntrySource::from_opener(source.size, move || {
                        let reader =
                            fiex_plugin_sdk::files::OwnedFileReader::open(proxy.clone(), &path)
                                .map_err(rars::Error::from)?;
                        Ok(Box::new(reader))
                    });
                    builder
                        .add_source(source.name.as_bytes().to_vec(), input, None, None)
                        .map_err(err)?;
                }
            }
            let mut writer = writer;
            builder
                .write_to(&mut writer, &rars::WriterResources::default(), None)
                .map_err(err)?;
            writer
        }
        _ => return Err("不支持的压缩格式".into()),
    };
    writer.commit().map_err(err)?;
    Ok(destination.into())
}
fn extract_file(
    files: &FileSystem<'_>,
    root: &Path,
    name: &Path,
    directory: bool,
    input: &mut dyn Read,
    remaining: &mut u64,
) -> Result<(), String> {
    safe_name(name)?;
    let path = root.join(name);
    if directory {
        return files.create_dir_all(&text(&path)).map_err(err);
    }
    if let Some(parent) = path.parent() {
        files.create_dir_all(&text(parent)).map_err(err)?;
    }
    let mut output = files.begin_write(&text(&path), true).map_err(err)?;
    let mut block = [0u8; 65536];
    loop {
        let n = input.read(&mut block).map_err(err)?;
        if n == 0 {
            break;
        }
        if n as u64 > *remaining {
            return Err("展开内容超过 20 GiB 限制".into());
        }
        output.write_all(&block[..n]).map_err(err)?;
        *remaining -= n as u64;
    }
    output.commit().map_err(err)
}
pub fn extract(
    source: &str,
    destination: Option<&str>,
    password: Option<&str>,
    host: &mut dyn HostServices,
) -> Result<String, String> {
    let proxy = host.worker_proxy();
    let files = host.files();
    let source_path = Path::new(source);
    let base = destination
        .map(Path::new)
        .unwrap_or_else(|| source_path.parent().unwrap_or(Path::new(".")));
    let target = unique(
        &files,
        &base.join(source_path.file_stem().ok_or("无效文件名")?),
    )?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(err)?
        .as_nanos();
    let root = base.join(format!(".fiex-extract-{stamp:x}"));
    files.create_dir_all(&text(&root)).map_err(err)?;
    let result = (|| {
        let lower = source.to_lowercase();
        let mut remaining = MAX_TOTAL;
        if lower.ends_with(".zip") {
            let mut archive =
                zip::ZipArchive::new(files.open_read(source).map_err(err)?).map_err(err)?;
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
                let dir = input.is_dir();
                extract_file(&files, &root, &name, dir, &mut input, &mut remaining)?;
            }
        } else if lower.ends_with(".7z") {
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
                    extract_file(
                        &files,
                        &root,
                        Path::new(&meta.name),
                        meta.is_directory,
                        input,
                        &mut remaining,
                    )
                    .map_err(|e| sevenz_rust2::Error::from(io::Error::other(e)))?;
                    Ok(true)
                })
                .map_err(err)?;
        } else if lower.ends_with(".rar") {
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
            let proxy = proxy.clone().ok_or("RAR requires a host worker session")?;
            let reader = fiex_plugin_sdk::files::OwnedFileReader::open(proxy.clone(), source)
                .map_err(err)?;
            let mut pending: Option<Arc<Mutex<Option<fiex_plugin_sdk::files::OwnedFileWriter>>>> =
                None;
            let mut count = 0;
            let mut options = rars::ArchiveReadOptions::default();
            options.password = password.map(str::as_bytes);
            options.max_total_output_bytes = Some(MAX_TOTAL);
            options.max_reader_workspace_bytes = Some(256 * 1024 * 1024);
            options.max_header_count = Some(MAX_ENTRIES as u64 + 1024);
            options.max_header_bytes = Some(64 * 1024 * 1024);
            let archive =
                rars::ArchiveReader::read_reader_with_options(reader, options).map_err(err)?;
            archive
                .extract_to_with_options(options, |meta| {
                    if let Some(previous) = pending.take() {
                        previous
                            .lock()
                            .map_err(|_| {
                                rars::Error::from(io::Error::other("writer lock poisoned"))
                            })?
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
                    safe_name(Path::new(&name))
                        .map_err(|e| rars::Error::from(io::Error::other(e)))?;
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
                        fiex_plugin_sdk::files::OwnedFileWriter::create(
                            proxy.clone(),
                            &text(&path),
                        )
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
        } else {
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
                    extract_file(
                        &files,
                        &root,
                        &name,
                        kind.is_dir(),
                        &mut input,
                        &mut remaining,
                    )?;
                }
            } else {
                extract_file(
                    &files,
                    &root,
                    Path::new(source_path.file_stem().ok_or("无效文件名")?),
                    false,
                    &mut { reader },
                    &mut remaining,
                )?;
            }
        }
        files.rename(&text(&root), &text(&target)).map_err(err)?;
        Ok(text(&target))
    })();
    if result.is_err() {
        let _ = files.remove(&text(&root));
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn traversal_and_platform_paths_are_rejected() {
        for path in ["../escape", "/absolute", "C:\\evil", "a/../../b", "a\\b"] {
            assert!(safe_name(Path::new(path)).is_err());
        }
        assert!(safe_name(Path::new("folder/hello.txt")).is_ok());
    }
}
