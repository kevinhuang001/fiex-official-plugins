//! Extraction entry point: stage into a hidden sibling directory, delegate the
//! per-format reading, then move the result to a non-colliding target.
use super::{
    err, formats,
    paths::{entry_ref, unique},
    ArchiveContext,
};
use crate::ignores::IgnoreSet;
use fiex_plugin_sdk::{Files, ResourceRef};
use std::{
    io::{Read, Write},
    path::Path,
};

pub(crate) fn extract_file(
    files: &Files,
    context: &dyn ArchiveContext,
    root: &ResourceRef,
    name: &Path,
    directory: bool,
    input: &mut dyn Read,
    remaining: &mut u64,
) -> Result<(), String> {
    let reference = entry_ref(files, root, name, directory)?;
    if directory {
        return Ok(());
    }
    let mut output = files.begin_write(&reference, true).map_err(err)?;
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
    source: &ResourceRef,
    destination: &ResourceRef,
    password: Option<&str>,
    ignores: &[String],
    max_total: u64,
    context: &dyn ArchiveContext,
) -> Result<ResourceRef, String> {
    let files = context.files();
    let metadata = files.metadata(source).map_err(err)?;
    let source_path = Path::new(&metadata.name);
    let base = destination;
    let target = unique(
        &files,
        base,
        &source_path
            .file_stem()
            .ok_or("无效文件名")?
            .to_string_lossy(),
    )?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(err)?
        .as_nanos();
    let root = files
        .child(base, &format!(".fiex-extract-{stamp:x}"))
        .map_err(err)?;
    files.create_dir(&root).map_err(err)?;
    let ignore = IgnoreSet::new(ignores);
    let result = (|| {
        let lower = metadata.name.to_lowercase();
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
        files.rename(&root, &target).map_err(err)?;
        Ok(target.clone())
    })();
    if result.is_err() {
        let _ = files.remove(&root);
    }
    result
}
