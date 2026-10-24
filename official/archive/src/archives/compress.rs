//! Compression entry point: collect the sources to store, then hand them to the
//! writer for the requested format.
use super::{err, formats, paths::text, MAX_ENTRIES};
use crate::ignores::IgnoreSet;
use fiex_plugin_sdk::{files::FileSystem, HostServices};
use std::path::Path;

pub(crate) struct Source {
    pub(crate) path: String,
    pub(crate) name: String,
    pub(crate) dir: bool,
    pub(crate) size: u64,
}

pub(crate) fn sources(
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
        "zip" => formats::write_zip(&files, &sources, writer, level, password)?,
        "tar" => formats::write_tar(&files, &sources, writer)?,
        "tar.gz" => formats::write_tar_gz(&files, &sources, writer, level)?,
        "tar.bz2" => formats::write_tar_bz2(&files, &sources, writer, level)?,
        "tar.xz" => formats::write_tar_xz(&files, &sources, writer, level)?,
        "7z" => formats::write_7z(&files, &sources, writer)?,
        "rar" => formats::write_rar(&sources, writer, level, password, proxy)?,
        _ => return Err("不支持的压缩格式".into()),
    };
    writer.commit().map_err(err)?;
    Ok(destination.into())
}
