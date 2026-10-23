//! Git history logic lives in the plugin; Core only executes authorized commands.
use crate::{
    files::{Entry, Listing},
    git_host::run,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use fiex_plugin_sdk::HostServices;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
#[derive(Clone)]
pub struct Route {
    pub root: PathBuf,
    pub commit: String,
    pub relative: String,
}
pub fn route(path: &str) -> Option<Route> {
    let mut parts = path.strip_prefix("history://")?.splitn(3, '/');
    let root = String::from_utf8(URL_SAFE_NO_PAD.decode(parts.next()?).ok()?).ok()?;
    let commit = parts.next()?;
    if ![40, 64].contains(&commit.len()) || !commit.bytes().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let relative = parts.next().unwrap_or("");
    if relative.contains(['\\', '\0']) || relative.split('/').any(|s| s == ".." || s == ".") {
        return None;
    }
    Some(Route {
        root: root.into(),
        commit: commit.into(),
        relative: relative.into(),
    })
}
pub fn history(root: &str, skip: usize, host: &mut dyn HostServices) -> Result<Value, String> {
    if run(host, root, &["rev-parse", "--verify", "HEAD"]).is_err() {
        return Ok(json!([]));
    }
    let skip = format!("--skip={skip}");
    let value = run(
        host,
        root,
        &[
            "log",
            "--all",
            "--date-order",
            "-50",
            &skip,
            "--format=%H%x00%h%x00%s%x00%an%x00%at%x00%D%x00",
        ],
    )?;
    let fields = value.split('\0').collect::<Vec<_>>();
    let entries=fields.chunks(6).filter(|p|p.len()==6).map(|p|json!({"id":p[0].trim_start_matches('\n'),"shortId":p[1],"subject":p[2],"author":p[3],"timestamp":p[4].parse::<u64>().unwrap_or(0),"refs":p[5]})).collect::<Vec<_>>();
    Ok(json!(entries))
}
pub fn browse(root: &str, reference: &str, host: &mut dyn HostServices) -> Result<Value, String> {
    if reference.is_empty() || reference.starts_with('-') {
        return Err("无效版本".into());
    }
    let full = run(host, root, &["rev-parse", "--show-toplevel"])?;
    let object = format!("{reference}^{{commit}}");
    let hash = run(
        host,
        root,
        &["rev-parse", "--verify", "--end-of-options", &object],
    )?;
    Ok(json!(format!(
        "history://{}/{}",
        URL_SAFE_NO_PAD.encode(full.trim().as_bytes()),
        hash.trim()
    )))
}
fn entries(
    path: &str,
    hidden: bool,
    ignores: &[String],
    recursive: bool,
    host: &mut dyn HostServices,
) -> Result<Vec<Entry>, String> {
    let r = route(path).ok_or("无效历史路径")?;
    let object = if r.relative.is_empty() {
        r.commit.clone()
    } else {
        format!("{}:{}", r.commit, r.relative)
    };
    let mut args = vec!["ls-tree", "-z", "-l"];
    if recursive {
        args.push("-r");
    }
    args.push(&object);
    let output = run(host, &r.root.to_string_lossy(), &args)?;
    let ignored = crate::ignores::IgnoreSet::new(ignores);
    let mut values = Vec::new();
    for record in output.split('\0').filter(|s| !s.is_empty()) {
        let Some((meta, name)) = record.split_once('\t') else {
            continue;
        };
        let mut meta = meta.split_whitespace();
        let mode = meta.next().unwrap_or("");
        let kind = meta.next().unwrap_or("");
        meta.next();
        let size = meta.next().and_then(|n| n.parse::<u64>().ok()).unwrap_or(0);
        if (!hidden && name.split('/').any(|n| n.starts_with('.')))
            || ignored.is_ignored(Path::new(name))
        {
            continue;
        }
        let is_dir = kind == "tree";
        let extension = Path::new(name)
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        values.push(Entry {
            path: format!("{}/{}", path.trim_end_matches('/'), name),
            name: name.into(),
            is_dir,
            is_symlink: mode == "120000",
            size,
            modified: 0,
            created: 0,
            extension,
            mime: if is_dir {
                "inode/directory".into()
            } else {
                mime_guess::from_path(name)
                    .first_or_octet_stream()
                    .to_string()
            },
            readonly: true,
            link_target: None,
            tags: vec![],
            sync: None,
        });
    }
    Ok(values)
}
pub fn list(
    path: &str,
    hidden: bool,
    ignores: &[String],
    host: &mut dyn HostServices,
) -> Result<Listing, String> {
    Ok(Listing {
        read_only: true,
        virtual_volume: true,
        path: path.into(),
        entries: entries(path, hidden, ignores, false, host)?,
        warnings: vec![],
    })
}
pub fn info(path: &str, host: &mut dyn HostServices) -> Result<Entry, String> {
    let (parent, _) = path.rsplit_once('/').ok_or("无效历史路径")?;
    entries(parent, true, &[], false, host)?
        .into_iter()
        .find(|v| v.path == path)
        .ok_or("历史文件不存在".into())
}
fn blob(path: &str, host: &mut dyn HostServices) -> Result<Vec<u8>, String> {
    let r = route(path).ok_or("无效历史路径")?;
    let object = format!("{}:{}", r.commit, r.relative);
    let result = host
        .process_run(
            "git",
            &[
                "-C".into(),
                r.root.to_string_lossy().into_owned(),
                "show".into(),
                object,
            ],
            None,
        )
        .map_err(|e| e.to_string())?;
    if result.code != Some(0) || result.truncated {
        return Err("无法读取历史内容，或内容超过命令输出限制".into());
    }
    result.stdout_bytes()
}
pub fn preview(path: &str, host: &mut dyn HostServices) -> Result<String, String> {
    let mut bytes = blob(path, host)?;
    bytes.truncate(256 * 1024);
    if bytes.contains(&0) {
        return Err("二进制内容不支持文本预览".into());
    }
    String::from_utf8(bytes).map_err(|e| e.to_string())
}
pub fn asset(path: &str, host: &mut dyn HostServices) -> Result<Value, String> {
    use base64::engine::general_purpose::STANDARD;
    let name = Path::new(path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    let bytes = blob(path, host)?;
    host.invoke(
        "host.preview.create",
        json!({"name":name,"base64":STANDARD.encode(bytes)}),
    )
    .map_err(|e| e.to_string())
}
pub fn search(
    path: &str,
    query: &str,
    hidden: bool,
    ignores: &[String],
    host: &mut dyn HostServices,
) -> Result<Vec<Entry>, String> {
    let query = query.to_lowercase();
    Ok(entries(path, hidden, ignores, true, host)?
        .into_iter()
        .filter(|v| v.name.to_lowercase().contains(&query))
        .take(2000)
        .collect())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn routes_reject_traversal() {
        let base = format!(
            "history://{}/{}",
            URL_SAFE_NO_PAD.encode("/repo"),
            "a".repeat(40)
        );
        assert!(route(&base).is_some());
        assert!(route(&format!("{base}/../secret")).is_none());
        assert!(route(&format!("{base}/a\\b")).is_none());
    }
}
