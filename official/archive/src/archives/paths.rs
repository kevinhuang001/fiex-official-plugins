//! Path helpers: safe extraction names, non-colliding targets and the default
//! archive destination.
use fiex_plugin_sdk::{files::FileSystem, HostServices};
use std::path::{Component, Path, PathBuf};

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

pub(crate) fn unique(files: &FileSystem<'_>, path: &Path) -> Result<PathBuf, String> {
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
