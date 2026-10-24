//! Extraction entry point: stage into a hidden sibling directory, delegate the
//! per-format reading, then move the result to a non-colliding target.
use super::{
    err, formats,
    paths::{safe_name, text, unique},
    ArchiveContext,
};
use crate::ignores::IgnoreSet;
use fiex_plugin_sdk::Files;
use std::{
    io::{Read, Write},
    path::Path,
};

pub(crate) fn extract_file(
    files: &Files,
    context: &dyn ArchiveContext,
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
        if context.cancelled() {
            return Err(super::CANCELLED.into());
        }
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
    ignores: &[String],
    max_total: u64,
    context: &dyn ArchiveContext,
) -> Result<String, String> {
    let files = context.files();
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
    let ignore = IgnoreSet::new(ignores);
    let result = (|| {
        let lower = source.to_lowercase();
        let mut remaining = max_total;
        if lower.ends_with(".zip") {
            formats::extract_zip(context, &root, source, password, &ignore, &mut remaining)?;
        } else if lower.ends_with(".7z") {
            formats::extract_7z(context, &root, source, password, &ignore, &mut remaining)?;
        } else if lower.ends_with(".rar") {
            formats::extract_rar(context, &root, source, password, &ignore, max_total)?;
        } else {
            formats::extract_stream(
                context,
                &root,
                source,
                source_path,
                &lower,
                &ignore,
                &mut remaining,
            )?;
        }
        files.rename(&text(&root), &text(&target)).map_err(err)?;
        Ok(text(&target))
    })();
    if result.is_err() {
        let _ = files.remove(&text(&root));
    }
    result
}
