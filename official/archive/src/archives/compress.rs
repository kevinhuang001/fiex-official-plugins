//! Compression entry point: collect the sources to store, then hand them to the
//! writer for the requested format.
use super::{err, formats, paths::text, ArchiveContext, MAX_ENTRIES};
use crate::ignores::IgnoreSet;
use fiex_plugin_sdk::{Files, ResourceRef};
use std::path::Path;

pub(crate) struct Source {
    pub(crate) path: ResourceRef,
    pub(crate) name: String,
    pub(crate) dir: bool,
    pub(crate) size: u64,
}

pub(crate) fn sources(
    files: &Files,
    paths: &[ResourceRef],
    ignore: &IgnoreSet,
) -> Result<Vec<Source>, String> {
    fn visit(
        files: &Files,
        path: &ResourceRef,
        name: &Path,
        ignore: &IgnoreSet,
        out: &mut Vec<Source>,
    ) -> Result<(), String> {
        if ignore.is_ignored(name) {
            return Ok(());
        }
        if out.len() >= MAX_ENTRIES || name.components().count() > 128 {
            return Err(crate::i18n::t("文件数量超过限制").into());
        }
        let meta = files.metadata(path).map_err(err)?;
        if meta.kind == "symlink" {
            return Err(crate::i18n::t("不跟随符号链接").into());
        }
        out.push(Source {
            path: path.clone(),
            name: text(name).replace('\\', "/"),
            dir: meta.kind == "directory",
            size: meta.size.unwrap_or(0),
        });
        if meta.kind == "directory" {
            for child in files.list(path).map_err(err)? {
                visit(
                    files,
                    &child.reference,
                    &name.join(child.name().ok_or("missing child metadata")?),
                    ignore,
                    out,
                )?;
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    for path in paths {
        let metadata = files.metadata(path).map_err(err)?;
        visit(files, path, Path::new(&metadata.name), ignore, &mut out)?;
    }
    Ok(out)
}

pub fn compress(
    paths: &[ResourceRef],
    destination: &ResourceRef,
    format: &str,
    ignores: &[String],
    level: Option<u32>,
    password: Option<&str>,
    context: &dyn ArchiveContext,
) -> Result<ResourceRef, String> {
    if paths.is_empty() {
        return Err(crate::i18n::t("请选择文件").into());
    }
    let files = context.files();
    let sources = sources(&files, paths, &IgnoreSet::new(ignores))?;
    let writer = files.begin_write(destination, true).map_err(err)?;
    let level = level.unwrap_or(6).min(9);
    let password = password.filter(|p| !p.is_empty());
    if password.is_some() && !matches!(format, "zip" | "rar") {
        return Err(crate::i18n::t("只有 ZIP 和 RAR 支持密码").into());
    }
    let writer = match format {
        "zip" => formats::write_zip(context, &sources, writer, level, password)?,
        "tar" => formats::write_tar(context, &sources, writer)?,
        "tar.gz" => formats::write_tar_gz(context, &sources, writer, level)?,
        "tar.bz2" => formats::write_tar_bz2(context, &sources, writer, level)?,
        "tar.xz" => formats::write_tar_xz(context, &sources, writer, level)?,
        "7z" => formats::write_7z(context, &sources, writer)?,
        "rar" => formats::write_rar(context, &sources, writer, level, password)?,
        _ => return Err(crate::i18n::t("不支持的压缩格式").into()),
    };
    writer.commit().map_err(err)?;
    Ok(destination.clone())
}
