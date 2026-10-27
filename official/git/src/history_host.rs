//! Git history logic lives in the plugin; Core only executes authorized commands.
use crate::{
    files::{Entry, Listing},
    git_host::run,
};

use fiex_plugin_sdk as sdk;
use serde_json::{json, Value};
use std::path::Path;
#[derive(Clone)]
pub struct Route {
    pub root: sdk::ResourceRef,
    pub commit: String,
    pub relative: String,
}
pub fn route(relative: &str, context: &Value) -> Option<Route> {
    let root = serde_json::from_value::<sdk::ResourceRef>(context["repoRoot"].clone()).ok()?;
    let commit = context["commitId"].as_str()?;
    if ![40, 64].contains(&commit.len())
        || !commit.bytes().all(|c| c.is_ascii_hexdigit())
    {
        return None;
    }
    if relative.starts_with('/')
        || relative.contains(['\\', '\0'])
        || relative.split('/').any(|s| s == ".." || s == ".")
    {
        return None;
    }
    Some(Route {
        root,
        commit: commit.into(),
        relative: relative.into(),
    })
}
fn request_route(path: &str, host: &sdk::RequestContext) -> Result<Route, String> {
    route(path, &host.params()["viewContext"]).ok_or_else(|| "无效历史上下文".into())
}
/// Providers return opaque node identities; only Core constructs navigation URIs.
pub fn provider_entry(entry: Entry) -> Value {
    let node = entry.path.clone();
    let mut value = serde_json::to_value(entry).expect("entry serializes");
    value["nodeId"] = json!(node);
    value["relativePath"] = json!(node);
    value
}
pub fn history(root: &sdk::ResourceRef, skip: usize, host: &sdk::RequestContext) -> Result<Value, String> {
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
pub fn browse(root: &sdk::ResourceRef, reference: &str, host: &sdk::RequestContext) -> Result<Value, String> {
    if reference.is_empty() || reference.starts_with('-') {
        return Err("无效版本".into());
    }
    
    let object = format!("{reference}^{{commit}}");
    let hash = run(
        host,
        root,
        &["rev-parse", "--verify", "--end-of-options", &object],
    )?;
    host.open_vfs_view(
        "history",
        root,
        json!({"repoRoot":root,"commitId":hash.trim()}),
    )
    .map_err(|e| e.to_string())
    .map(Value::String)
}
fn entries(
    path: &str,
    hidden: bool,
    ignores: &[String],
    recursive: bool,
    host: &sdk::RequestContext,
) -> Result<Vec<Entry>, String> {
    let r = request_route(path, host)?;
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
    let output = run(host, &r.root, &args)?;
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
            path: if path.is_empty() {
                name.into()
            } else {
                format!("{path}/{name}")
            },
            name: name.rsplit('/').next().unwrap_or(name).into(),
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
    host: &sdk::RequestContext,
) -> Result<Listing, String> {
    Ok(Listing {
        read_only: true,
        virtual_volume: true,
        path: path.into(),
        entries: entries(path, hidden, ignores, false, host)?,
        warnings: vec![],
    })
}
pub fn info(path: &str, host: &sdk::RequestContext) -> Result<Entry, String> {
    if path.is_empty() {
        let r = request_route(path, host)?;
        run(
            host,
            &r.root,
            &["cat-file", "-e", &format!("{}^{{commit}}", r.commit)],
        )?;
        return Ok(Entry {
            path: String::new(),
            name: r.commit,
            is_dir: true,
            is_symlink: false,
            size: 0,
            modified: 0,
            created: 0,
            extension: String::new(),
            mime: "inode/directory".into(),
            readonly: true,
            link_target: None,
            tags: vec![],
            sync: None,
        });
    }
    let parent = path.rsplit_once('/').map(|(p, _)| p).unwrap_or("");
    entries(parent, true, &[], false, host)?
        .into_iter()
        .find(|v| v.path == path)
        .ok_or("历史文件不存在".into())
}
pub fn blob(path: &str, host: &sdk::RequestContext) -> Result<Vec<u8>, String> {
    let r = request_route(path, host)?;
    let object = format!("{}:{}", r.commit, r.relative);
    let result = host
        .process_run_at(
            &r.root, "git",
            &["show".into(), object],
            None,
        )
        .map_err(|e| e.to_string())?;
    if result.code != Some(0) || result.truncated {
        return Err("无法读取历史内容，或内容超过命令输出限制".into());
    }
    result.stdout_bytes()
}
pub fn search(
    path: &str,
    query: &str,
    hidden: bool,
    ignores: &[String],
    host: &sdk::RequestContext,
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
    fn routes_are_immutable_and_reject_traversal() {
        let a =
            json!({"repoRoot":std::env::temp_dir().to_string_lossy(),"commitId":"a".repeat(40)});
        let b =
            json!({"repoRoot":std::env::temp_dir().to_string_lossy(),"commitId":"b".repeat(40)});
        assert_ne!(
            route("src/x", &a).unwrap().commit,
            route("src/x", &b).unwrap().commit
        );
        for path in ["../secret", "a\\b", "/absolute", "a/./b"] {
            assert!(route(path, &a).is_none());
        }
        assert!(route("", &json!({"repoRoot":"relative","commitId":"HEAD"})).is_none());
    }
}
