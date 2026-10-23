use git2::{
    build::CheckoutBuilder, IndexAddOption, Repository, RepositoryInitOptions, Status,
    StatusOptions,
};
use serde::Serialize;
use std::{collections::HashMap, path::Path};

#[derive(Serialize)]
pub struct GitStatus {
    pub root: String,
    pub branch: String,
    pub entries: HashMap<String, String>,
}

fn open(path: &str) -> Option<Repository> {
    Repository::discover(path).ok()
}

fn status_code(status: Status) -> Option<String> {
    if status.contains(Status::CONFLICTED) {
        return Some("UU".into());
    }
    if status.contains(Status::WT_NEW) {
        return Some("??".into());
    }
    let mut code = [' ', ' '];
    let index = if status.contains(Status::INDEX_NEW) {
        'A'
    } else if status.contains(Status::INDEX_MODIFIED) {
        'M'
    } else if status.contains(Status::INDEX_DELETED) {
        'D'
    } else if status.contains(Status::INDEX_RENAMED) {
        'R'
    } else if status.contains(Status::INDEX_TYPECHANGE) {
        'T'
    } else {
        ' '
    };
    let work = if status.contains(Status::WT_MODIFIED) {
        'M'
    } else if status.contains(Status::WT_DELETED) {
        'D'
    } else if status.contains(Status::WT_RENAMED) {
        'R'
    } else if status.contains(Status::WT_TYPECHANGE) {
        'T'
    } else {
        ' '
    };
    code[0] = index;
    code[1] = work;
    (code != [' ', ' ']).then(|| code.iter().collect())
}

/// Returns branch and per-file status when `path` is inside a work tree.
pub fn status(path: &str) -> Option<GitStatus> {
    if let Some(route) = crate::git_history::route(path) {
        return Some(GitStatus {
            root: route.root.to_string_lossy().into_owned(),
            branch: route.commit[..8].into(),
            entries: HashMap::new(),
        });
    }
    let repo = open(path)?;
    let root = dunce::canonicalize(repo.workdir()?).ok()?;
    let branch = repo
        .head()
        .ok()
        .and_then(|head| head.shorthand().ok().map(str::to_owned))
        .filter(|name| !name.is_empty())
        .or_else(|| {
            repo.find_reference("HEAD")
                .ok()
                .and_then(|head| head.symbolic_target().ok().flatten().map(str::to_owned))
                .map(|name| name.trim_start_matches("refs/heads/").to_owned())
        })
        .unwrap_or_else(|| "HEAD".into());
    let mut options = StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .renames_head_to_index(true)
        .renames_index_to_workdir(true)
        .include_ignored(false);
    let statuses = repo.statuses(Some(&mut options)).ok()?;
    let mut entries = HashMap::new();
    for entry in statuses.iter() {
        let Some(code) = status_code(entry.status()) else {
            continue;
        };
        let Some(path) = entry.path().ok() else {
            continue;
        };
        entries.insert(root.join(path).to_string_lossy().into_owned(), code);
    }
    Some(GitStatus {
        root: root.to_string_lossy().into_owned(),
        branch,
        entries,
    })
}

#[derive(Serialize)]
pub struct GitRefs {
    pub head: String,
    pub current: String,
    pub branches: Vec<String>,
    pub tags: Vec<String>,
}

pub fn refs(root: &str) -> Result<GitRefs, String> {
    let repo = open(root).ok_or("无法打开 Git 仓库")?;
    let current = repo
        .head()
        .ok()
        .and_then(|head| head.shorthand().ok().map(str::to_owned))
        .filter(|name| !name.is_empty())
        .or_else(|| {
            repo.find_reference("HEAD")
                .ok()
                .and_then(|head| head.symbolic_target().ok().flatten().map(str::to_owned))
                .map(|name| name.trim_start_matches("refs/heads/").to_owned())
        })
        .unwrap_or_else(|| "HEAD".into());
    let head = repo
        .head()
        .ok()
        .and_then(|head| head.peel_to_commit().ok())
        .map(|commit| commit.id().to_string()[..7].to_string())
        .unwrap_or_default();
    let mut branches = Vec::new();
    if let Ok(iter) = repo.branches(Some(git2::BranchType::Local)) {
        for branch in iter.flatten() {
            if let Some(name) = branch.0.name().ok().flatten() {
                branches.push(name.to_string());
            }
        }
    }
    let mut tags = Vec::new();
    if let Ok(names) = repo.tag_names(None) {
        for name in names.iter() {
            if let Ok(Some(name)) = name {
                tags.push(name.to_owned());
            }
        }
    }
    branches.sort();
    tags.sort();
    Ok(GitRefs {
        head,
        current,
        branches,
        tags,
    })
}

pub fn init(path: &str) -> Result<GitStatus, String> {
    let path = dunce::canonicalize(path).map_err(|error| error.to_string())?;
    if !path.is_dir() {
        return Err("请选择本地文件夹来创建仓库".into());
    }
    if Repository::discover(&path).is_ok() {
        return status(&path.to_string_lossy()).ok_or_else(|| "无法读取仓库状态".into());
    }
    let mut options = RepositoryInitOptions::new();
    options.initial_head("main");
    Repository::init_opts(&path, &options).map_err(|error| error.to_string())?;
    status(&path.to_string_lossy()).ok_or_else(|| "仓库已创建，但无法读取状态".into())
}

pub(crate) fn repository(root: &str) -> Result<Repository, String> {
    Repository::discover(root).map_err(|error| error.to_string())
}

fn relative_paths(root: &Path, paths: &[String]) -> Result<Vec<String>, String> {
    paths
        .iter()
        .map(|value| {
            let path = Path::new(value);
            if !path.is_absolute()
                || path
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                return Err("文件路径无效".into());
            }
            let mut ancestor = path.parent().ok_or("文件路径无效")?;
            let mut tail = vec![path.file_name().ok_or("文件路径无效")?.to_os_string()];
            while !ancestor.exists() {
                tail.push(ancestor.file_name().ok_or("文件路径无效")?.to_os_string());
                ancestor = ancestor.parent().ok_or("文件路径无效")?;
            }
            let mut resolved = dunce::canonicalize(ancestor).map_err(|e| e.to_string())?;
            for part in tail.into_iter().rev() {
                resolved.push(part);
            }
            let relative = resolved
                .strip_prefix(root)
                .map_err(|_| "文件不在当前仓库中")?;
            if relative.as_os_str().is_empty() {
                return Err("文件路径无效".into());
            }
            Ok(relative.to_string_lossy().into_owned())
        })
        .collect()
}

pub fn stage(root: &str, paths: &[String], all: bool) -> Result<(), String> {
    let repo = repository(root)?;
    let workdir = repo.workdir().ok_or("仓库没有工作区")?.to_path_buf();
    let mut index = repo.index().map_err(|error| error.to_string())?;
    if all {
        index
            .add_all(["*"], IndexAddOption::DEFAULT, None)
            .map_err(|error| error.to_string())?;
    } else {
        for path in relative_paths(&workdir, paths)? {
            index
                .add_path(Path::new(&path))
                .map_err(|error| error.to_string())?;
        }
    }
    index.write().map_err(|error| error.to_string())
}

pub fn unstage(root: &str, paths: &[String], all: bool) -> Result<(), String> {
    let repo = repository(root)?;
    let workdir = repo.workdir().ok_or("仓库没有工作区")?.to_path_buf();
    let paths = relative_paths(&workdir, paths)?;
    if !all && paths.is_empty() {
        return Err("请选择要取消暂存的文件".into());
    }
    let head = repo.head().ok().and_then(|head| head.peel_to_commit().ok());
    if let Some(head) = head {
        let mut checkout = CheckoutBuilder::new();
        checkout.safe();
        repo.reset_default(
            Some(head.as_object()),
            if all {
                vec!["*"]
            } else {
                paths.iter().map(String::as_str).collect()
            },
        )
        .map_err(|error| error.to_string())?;
    } else {
        let mut index = repo.index().map_err(|error| error.to_string())?;
        if all {
            index.clear().map_err(|error| error.to_string())?;
        } else {
            for path in &paths {
                index
                    .remove_path(Path::new(path))
                    .map_err(|error| error.to_string())?;
            }
        }
        index.write().map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[derive(Serialize)]
pub struct GitIdentity {
    pub name: String,
    pub email: String,
}
pub fn identity(root: &str) -> GitIdentity {
    let repo = open(root);
    let get = |key: &str| {
        repo.as_ref()
            .and_then(|repo| repo.config().ok())
            .and_then(|config| config.get_string(key).ok())
            .unwrap_or_default()
    };
    GitIdentity {
        name: get("user.name"),
        email: get("user.email"),
    }
}

pub fn commit(root: &str, message: &str, name: &str, email: &str) -> Result<String, String> {
    let repo = repository(root)?;
    let message = message.trim();
    if message.is_empty() {
        return Err("请填写提交说明".into());
    }
    if name.trim().is_empty() || email.trim().is_empty() {
        return Err("请填写提交者姓名和邮箱".into());
    }
    {
        let mut config = repo.config().map_err(|error| error.to_string())?;
        config
            .set_str("user.name", name.trim())
            .map_err(|error| error.to_string())?;
        config
            .set_str("user.email", email.trim())
            .map_err(|error| error.to_string())?;
    }
    let mut index = repo.index().map_err(|error| error.to_string())?;
    let tree_id = index.write_tree().map_err(|error| error.to_string())?;
    let tree = repo.find_tree(tree_id).map_err(|error| error.to_string())?;
    let signature =
        git2::Signature::now(name.trim(), email.trim()).map_err(|error| error.to_string())?;
    let head = repo.head().ok().and_then(|head| head.peel_to_commit().ok());
    let parents = head.iter().collect::<Vec<_>>();
    let parent_refs = parents.to_vec();
    let oid = repo
        .commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &parent_refs,
        )
        .map_err(|error| error.to_string())?;
    Ok(oid.to_string()[..7].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_stage_unstage_first_commit_and_modify() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_str().unwrap();
        std::fs::write(temp.path().join("notes.txt"), "first").unwrap();
        let initialized = init(root).unwrap();
        assert_eq!(initialized.branch, "main");
        stage(root, &[], true).unwrap();
        unstage(root, &[], true).unwrap();
        assert!(temp.path().join("notes.txt").exists());
        assert!(status(root).unwrap().entries.values().any(|v| v == "??"));
        stage(root, &[], true).unwrap();
        assert!(
            !commit(root, "First commit", "Fiex Test", "fiex@example.com")
                .unwrap()
                .is_empty()
        );
        assert_eq!(identity(root).name, "Fiex Test");
        assert!(status(root).unwrap().entries.is_empty());
        std::fs::write(temp.path().join("notes.txt"), "second").unwrap();
        stage(root, &[], true).unwrap();
        unstage(root, &[], true).unwrap();
        assert!(status(root).unwrap().entries.values().any(|v| v == " M"));
    }

    #[test]
    fn paths_are_literal_and_cannot_escape_repository() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_str().unwrap();
        init(root).unwrap();
        for name in ["[draft].txt", "draft.txt", "-option.txt"] {
            std::fs::write(temp.path().join(name), "x").unwrap();
        }
        stage(
            root,
            &[temp
                .path()
                .join("[draft].txt")
                .to_string_lossy()
                .into_owned()],
            false,
        )
        .unwrap();
        let state = status(root).unwrap();
        let canonical = dunce::canonicalize(temp.path()).unwrap();
        assert_eq!(
            state.entries[&canonical.join("[draft].txt").to_string_lossy().into_owned()],
            "A "
        );
        assert_eq!(
            state.entries[&canonical.join("draft.txt").to_string_lossy().into_owned()],
            "??"
        );
        assert!(stage(
            root,
            &[temp
                .path()
                .join("../outside")
                .to_string_lossy()
                .into_owned()],
            false,
        )
        .is_err());
        assert!(commit(root, " ", "Test", "test@example.com").is_err());
    }
}
