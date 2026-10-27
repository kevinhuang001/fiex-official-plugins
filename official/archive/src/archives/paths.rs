//! Path helpers: safe extraction names, non-colliding targets and the default
//! archive destination.
use fiex_plugin_sdk::{self as sdk, Files};
use std::path::{Component, Path};

pub(crate) fn safe_name(path: &Path) -> Result<(), String> {
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

pub(crate) fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

pub(crate) fn unique(
    files: &Files,
    parent: &sdk::ResourceRef,
    name: &str,
) -> Result<sdk::ResourceRef, String> {
    let names = files
        .list(parent)
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter_map(|entry| entry.metadata.map(|metadata| metadata.name))
        .collect::<std::collections::HashSet<_>>();
    for index in 0..1000 {
        let candidate = if index == 0 {
            name.to_string()
        } else {
            format!("{name} ({})", index + 1)
        };
        if !names.contains(&candidate) {
            return files
                .child(parent, &candidate)
                .map_err(|error| error.to_string());
        }
    }
    Err("无法生成不冲突的文件名".into())
}

pub(crate) fn entry_ref(
    files: &Files,
    root: &sdk::ResourceRef,
    name: &Path,
    directory: bool,
) -> Result<sdk::ResourceRef, String> {
    safe_name(name)?;
    let components = name
        .components()
        .filter_map(|component| {
            if let Component::Normal(name) = component {
                Some(name.to_string_lossy().into_owned())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    if components.is_empty() && !directory {
        return Err("无效文件名".into());
    }
    let mut current = root.clone();
    for (index, name) in components.iter().enumerate() {
        current = files
            .child(&current, name)
            .map_err(|error| error.to_string())?;
        if directory || index + 1 < components.len() {
            files
                .create_dir(&current)
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(current)
}

pub fn default_destination(
    paths: &[sdk::ResourceRef],
    format: &str,
    host: &sdk::RequestContext,
) -> Result<sdk::ResourceRef, String> {
    if !matches!(
        format,
        "zip" | "rar" | "7z" | "tar" | "tar.gz" | "tar.bz2" | "tar.xz"
    ) {
        return Err("不支持的压缩格式".into());
    }
    let parent = host.directory().ok_or("请选择输出文件夹")?.reference;
    let files = host.files();
    let metadata = files
        .metadata(paths.first().ok_or("请选择文件")?)
        .map_err(|error| error.to_string())?;
    let stem = if paths.len() == 1 {
        Path::new(&metadata.name)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    } else {
        "Archive".into()
    };
    unique(&files, &parent, &format!("{stem}.{format}"))
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
