//! Read-only Git tree browsing backed by git2. Never checks out or changes the
//! user's work tree.
use crate::files::{Entry, Listing};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use git2::{ObjectType, Repository, Sort, Tree};
use serde::Serialize;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub struct Route {
    pub root: PathBuf,
    pub commit: String,
    pub relative: String,
}
pub fn route(path: &str) -> Option<Route> {
    let tail = path.strip_prefix("history://")?;
    let mut parts = tail.splitn(3, '/');
    let encoded = parts.next()?;
    let commit = parts.next()?;
    if ![40, 64].contains(&commit.len()) || !commit.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let root = String::from_utf8(URL_SAFE_NO_PAD.decode(encoded).ok()?).ok()?;
    let relative = parts.next().unwrap_or("").to_string();
    if relative.split('/').any(|s| s == ".." || s == ".")
        || relative.contains('\0')
        || Path::new(&relative)
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return None;
    }
    Some(Route {
        root: PathBuf::from(root),
        commit: commit.into(),
        relative,
    })
}

fn repo(path: &Path) -> Result<Repository, String> {
    Repository::discover(path).map_err(|error| error.to_string())
}

fn commit_tree<'a>(
    repo: &'a Repository,
    route: &Route,
) -> Result<(git2::Commit<'a>, Tree<'a>), String> {
    let oid = git2::Oid::from_str(&route.commit).map_err(|error| error.to_string())?;
    let commit = repo.find_commit(oid).map_err(|error| error.to_string())?;
    let tree = commit.tree().map_err(|error| error.to_string())?;
    Ok((commit, tree))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    pub id: String,
    pub short_id: String,
    pub subject: String,
    pub author: String,
    pub timestamp: u64,
    pub refs: String,
}

pub fn history(root: &str, skip: usize) -> Result<Vec<Commit>, String> {
    let repo = Repository::discover(root).map_err(|error| error.to_string())?;
    let mut walk = repo.revwalk().map_err(|error| error.to_string())?;
    walk.set_sorting(Sort::TIME)
        .map_err(|error| error.to_string())?;
    let mut pushed = false;
    for pattern in ["refs/heads/*", "refs/tags/*"] {
        if walk.push_glob(pattern).is_ok() {
            pushed = true;
        }
    }
    if !pushed {
        let _ = walk.push_head();
    }
    let mut seen = HashSet::new();
    let mut commits = Vec::new();
    for oid in walk.flatten() {
        if !seen.insert(oid) {
            continue;
        }
        let Ok(commit) = repo.find_commit(oid) else {
            continue;
        };
        let timestamp = commit.time().seconds().max(0) as u64;
        let author = commit.author().name().unwrap_or_default().to_owned();
        let mut refs = Vec::new();
        if let Ok(references) = repo.references() {
            for reference in references.flatten() {
                if reference.target() == Some(oid)
                    || reference.peel_to_commit().ok().map(|c| c.id()) == Some(oid)
                {
                    if let Ok(name) = reference.shorthand() {
                        refs.push(name.to_owned());
                    }
                }
            }
        }
        refs.sort();
        commits.push(Commit {
            id: oid.to_string(),
            short_id: oid.to_string()[..7].to_string(),
            subject: commit.summary().ok().flatten().unwrap_or("").to_string(),
            author,
            timestamp,
            refs: refs.join(", "),
        });
    }
    commits.sort_by_key(|commit| std::cmp::Reverse(commit.timestamp));
    Ok(commits.into_iter().skip(skip).take(50).collect())
}

pub fn browse(root: &str, reference: &str) -> Result<String, String> {
    if reference.is_empty() || reference.starts_with('-') {
        return Err("请选择有效的版本".into());
    }
    let root = dunce::canonicalize(root).map_err(|error| error.to_string())?;
    let repo = repo(&root)?;
    let object = repo
        .revparse_single(reference)
        .map_err(|_| "无法解析提交".to_string())?;
    let commit = object
        .peel_to_commit()
        .map_err(|_| "无法解析提交".to_string())?;
    let encoded = URL_SAFE_NO_PAD.encode(root.to_string_lossy().as_bytes());
    Ok(format!("history://{encoded}/{}", commit.id()))
}

fn tree_entry<'a>(tree: &'a Tree<'a>, path: &str) -> Result<git2::TreeEntry<'a>, String> {
    if path.is_empty() {
        return Err("请选择文件".into());
    }
    tree.get_path(Path::new(path))
        .map_err(|_| "历史文件不存在".into())
}

pub fn list(path: &str, hidden: bool, ignores: &[String]) -> Result<Listing, String> {
    let route = route(path).ok_or("历史版本路径无效")?;
    let repo = repo(&route.root)?;
    let (commit, tree) = commit_tree(&repo, &route)?;
    let tree = if route.relative.is_empty() {
        tree
    } else {
        tree_entry(&tree, &route.relative)?
            .to_object(&repo)
            .map_err(|error| error.to_string())?
            .peel_to_tree()
            .map_err(|error| error.to_string())?
    };
    let ignored = crate::ignores::IgnoreSet::new(ignores);
    let stamp = commit.time().seconds().max(0) as u64;
    let mut entries = Vec::new();
    for entry in tree.iter() {
        let Some(name) = entry.name().ok() else {
            continue;
        };
        if (!hidden && name.starts_with('.')) || ignored.is_ignored(Path::new(name)) {
            continue;
        }
        let is_dir = entry.kind() == Some(ObjectType::Tree);
        let size = entry
            .to_object(&repo)
            .ok()
            .and_then(|object| object.as_blob().map(|blob| blob.size() as u64))
            .unwrap_or(0);
        let extension = if is_dir {
            String::new()
        } else {
            Path::new(name)
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase()
        };
        entries.push(Entry {
            path: format!("{}/{}", path.trim_end_matches('/'), name),
            name: name.to_owned(),
            is_dir,
            is_symlink: entry.filemode() == 0o120000,
            size,
            modified: stamp,
            created: stamp,
            extension,
            mime: mime_guess::from_path(name)
                .first_or_octet_stream()
                .to_string(),
            readonly: true,
            link_target: None,
            tags: vec![],
            sync: None,
        });
    }
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.cmp(&b.name)));
    Ok(Listing {
        read_only: true,
        virtual_volume: false,
        path: path.into(),
        entries,
        warnings: vec![],
    })
}

pub fn info(path: &str) -> Result<Entry, String> {
    let i = path.rfind('/').ok_or("无效路径")?;
    list(&path[..i], true, &[])?
        .entries
        .into_iter()
        .find(|entry| entry.path == path)
        .ok_or_else(|| "历史文件不存在".into())
}

fn blob(path: &str, limit: u64) -> Result<Vec<u8>, String> {
    let route = route(path).ok_or("历史版本路径无效")?;
    let repo = repo(&route.root)?;
    let (_, tree) = commit_tree(&repo, &route)?;
    let entry = tree_entry(&tree, &route.relative)?;
    let object = entry.to_object(&repo).map_err(|error| error.to_string())?;
    let blob = object
        .peel_to_blob()
        .map_err(|_| "历史文件不存在".to_string())?;
    let mut data = blob.content().to_vec();
    data.truncate(limit as usize);
    Ok(data)
}

pub fn preview(path: &str) -> Result<String, String> {
    let data = blob(path, 256 * 1024)?;
    if data.contains(&0) {
        return Err("二进制文件，请使用对应的预览方式".into());
    }
    String::from_utf8(data).map_err(|_| "文本不是 UTF-8 编码".into())
}

pub fn asset(path: &str) -> Result<String, String> {
    let info = info(path)?;
    if info.is_symlink || info.is_dir {
        return Err("此类型不提供媒体预览".into());
    }
    if info.size > 64 * 1024 * 1024 {
        return Err("历史媒体超过 64 MB，无法快速预览".into());
    }
    let route = route(path).ok_or("无效历史路径")?;
    let base = dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("fiex")
        .join("history-preview");
    let encoded = URL_SAFE_NO_PAD.encode(route.root.to_string_lossy().as_bytes());
    let target = base.join(encoded).join(&route.commit).join(&route.relative);
    if !target.is_file() {
        std::fs::create_dir_all(target.parent().ok_or("无效文件名")?).map_err(|e| e.to_string())?;
        std::fs::write(&target, blob(path, 64 * 1024 * 1024 + 1)?).map_err(|e| e.to_string())?;
        let mut permissions = std::fs::metadata(&target)
            .map_err(|e| e.to_string())?
            .permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&target, permissions).map_err(|e| e.to_string())?;
    }
    Ok(target.to_string_lossy().into_owned())
}

pub fn search(
    path: &str,
    query: &str,
    hidden: bool,
    ignores: &[String],
) -> Result<Vec<Entry>, String> {
    let mut pending = vec![path.to_string()];
    let mut matches = Vec::new();
    let query = query.to_lowercase();
    while let Some(directory) = pending.pop() {
        for entry in list(&directory, hidden, ignores)?.entries {
            if entry.is_dir {
                pending.push(entry.path.clone());
            }
            if entry.name.to_lowercase().contains(&query) {
                matches.push(entry);
                if matches.len() >= 500 {
                    return Ok(matches);
                }
            }
        }
    }
    Ok(matches)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_merges_branches_in_timestamp_order_even_with_skewed_dates() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_str().unwrap();
        crate::git::init(root).unwrap();
        assert!(history(root, 0).unwrap().is_empty());
        let commit = |message: &str, timestamp: i64| {
            let repo = Repository::open(root).unwrap();
            let mut index = repo.index().unwrap();
            let tree_id = index.write_tree().unwrap();
            let tree = repo.find_tree(tree_id).unwrap();
            let signature =
                git2::Signature::new("Test", "test@example.com", &git2::Time::new(timestamp, 0))
                    .unwrap();
            let head = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
            let parents = head.iter().collect::<Vec<_>>();
            let parent_refs = parents.to_vec();
            repo.commit(
                Some("HEAD"),
                &signature,
                &signature,
                message,
                &tree,
                &parent_refs,
            )
            .unwrap();
        };
        commit("Base", 1_000_000_000);
        let base = Repository::open(root)
            .unwrap()
            .head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id();
        commit("Older child on main", 900_000_000);
        let repo = Repository::open(root).unwrap();
        repo.branch("dev", &repo.find_commit(base).unwrap(), false)
            .unwrap();
        repo.set_head("refs/heads/dev").unwrap();
        commit("Newer commit on dev", 1_100_000_000);
        let commits = history(root, 0).unwrap();
        assert_eq!(
            commits
                .iter()
                .map(|c| c.subject.as_str())
                .collect::<Vec<_>>(),
            vec!["Newer commit on dev", "Base", "Older child on main"]
        );
        assert!(commits[0].refs.contains("dev"));
        assert!(history(root, 50).unwrap().is_empty());
    }

    #[test]
    fn browses_old_bytes_without_checking_out_or_modifying_worktree() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_str().unwrap();
        crate::git::init(root).unwrap();
        let file = temp.path().join("hello.txt");
        std::fs::write(&file, "old version").unwrap();
        crate::git::stage(root, &[], true).unwrap();
        let first = crate::git::commit(root, "First", "Test", "test@example.com").unwrap();
        std::fs::write(&file, "uncommitted version").unwrap();
        let path = browse(root, &first).unwrap();
        assert_eq!(list(&path, false, &[]).unwrap().entries.len(), 1);
        assert_eq!(
            preview(&format!("{path}/hello.txt")).unwrap(),
            "old version"
        );
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "uncommitted version"
        );
        assert_eq!(history(root, 0).unwrap().len(), 1);
        assert!(route(&format!("{path}/../escape")).is_none());
        assert!(route(&format!("{path}//absolute")).is_none());
        assert_eq!(crate::git::status(root).unwrap().branch, "main");
    }
}
