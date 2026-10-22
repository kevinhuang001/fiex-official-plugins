use crate::host::{git, text};
use fiex_plugin_sdk::HostServices;
use serde::Serialize;
use std::{collections::HashMap, path::Path};

#[derive(Serialize)]
pub struct GitStatus {
    pub root: String,
    pub branch: String,
    pub entries: HashMap<String, String>,
}

fn output(host: &mut dyn HostServices, root: &Path, args: &[&str]) -> Option<String> {
    let out = git(host, root, args).ok()?;
    if out.code != Some(0) {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Returns branch and per-file status when the host can run `git` and `path`
/// is in a work tree. Status keys are absolute canonical paths.
pub fn status(host: &mut dyn HostServices, path: &str) -> Option<GitStatus> {
    if let Some(route) = crate::git_history::route(path) {
        return Some(GitStatus {
            root: route.root.to_string_lossy().into_owned(),
            branch: route.commit[..8].into(),
            entries: HashMap::new(),
        });
    }
    let root = output(host, Path::new(path), &["rev-parse", "--show-toplevel"])?;
    let root = dunce::canonicalize(root).ok()?;
    let mut branch =
        output(host, &root, &["symbolic-ref", "--short", "HEAD"]).unwrap_or_else(|| "HEAD".into());
    if branch == "HEAD" {
        branch =
            output(host, &root, &["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "HEAD".into());
    }
    let raw = git(
        host,
        &root,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )
    .ok()?;
    if raw.code != Some(0) {
        return None;
    }
    let mut entries = HashMap::new();
    let mut fields = raw.stdout.split(|byte| *byte == 0);
    while let Some(field) = fields.next() {
        if field.len() < 4 {
            continue;
        }
        let xy = &field[..2];
        let relative = String::from_utf8_lossy(&field[3..]).replace('\\', "/");
        let code = String::from_utf8_lossy(xy).into_owned();
        let absolute = root.join(relative).to_string_lossy().into_owned();
        entries.insert(absolute, code);

        if xy.contains(&b'R') || xy.contains(&b'C') {
            let _ = fields.next();
        }
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

pub fn refs(host: &mut dyn HostServices, root: &str) -> Result<GitRefs, String> {
    let current = output(host, Path::new(root), &["symbolic-ref", "--short", "HEAD"])
        .unwrap_or_else(|| "HEAD".into());
    let branches = output(
        host,
        Path::new(root),
        &["for-each-ref", "--format=%(refname:short)", "refs/heads"],
    )
    .unwrap_or_default()
    .lines()
    .map(str::to_string)
    .filter(|value| !value.is_empty())
    .collect::<Vec<_>>();
    let tags = output(
        host,
        Path::new(root),
        &["for-each-ref", "--format=%(refname:short)", "refs/tags"],
    )
    .unwrap_or_default()
    .lines()
    .map(str::to_string)
    .filter(|value| !value.is_empty())
    .collect::<Vec<_>>();
    Ok(GitRefs {
        head: output(host, Path::new(root), &["rev-parse", "--short", "HEAD"]).unwrap_or_default(),
        current,
        branches,
        tags,
    })
}

pub(crate) fn run(
    host: &mut dyn HostServices,
    root: &Path,
    args: &[&str],
) -> Result<String, String> {
    let mut args_with_literal = vec!["--literal-pathspecs"];
    args_with_literal.extend_from_slice(args);
    text(host, root, &args_with_literal)
}

pub fn init(host: &mut dyn HostServices, path: &str) -> Result<GitStatus, String> {
    let path = dunce::canonicalize(path).map_err(|error| error.to_string())?;
    if !path.is_dir() {
        return Err("请选择本地文件夹来创建仓库".into());
    }
    if let Some(existing) = status(host, &path.to_string_lossy()) {
        return Ok(existing);
    }
    run(host, &path, &["init", "--initial-branch=main"])?;
    status(host, &path.to_string_lossy()).ok_or_else(|| "仓库已创建，但无法读取状态".into())
}

pub(crate) fn repository(
    host: &mut dyn HostServices,
    root: &str,
) -> Result<std::path::PathBuf, String> {
    let root = dunce::canonicalize(root).map_err(|error| error.to_string())?;
    let found = run(host, &root, &["rev-parse", "--show-toplevel"])?;
    dunce::canonicalize(found).map_err(|error| error.to_string())
}

pub fn stage(
    host: &mut dyn HostServices,
    root: &str,
    paths: &[String],
    all: bool,
) -> Result<(), String> {
    let root = repository(host, root)?;
    if all {
        run(host, &root, &["add", "--all", "--", "."])?;
        return Ok(());
    }
    let paths = relative_paths(&root, paths)?;
    if paths.is_empty() {
        return Err("请选择要暂存的文件".into());
    }
    let mut args = vec!["add", "--all", "--"];
    args.extend(paths.iter().map(String::as_str));
    run(host, &root, &args)?;
    Ok(())
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

pub fn unstage(
    host: &mut dyn HostServices,
    root: &str,
    paths: &[String],
    all: bool,
) -> Result<(), String> {
    let root = repository(host, root)?;
    let paths = relative_paths(&root, paths)?;
    if !all && paths.is_empty() {
        return Err("请选择要取消暂存的文件".into());
    }
    let has_head = run(host, &root, &["rev-parse", "--verify", "HEAD"]).is_ok();
    let mut args = if has_head {
        vec!["reset", "--quiet", "HEAD", "--"]
    } else {
        vec!["rm", "--cached", "-r", "--ignore-unmatch", "--"]
    };
    if all {
        args.push(".");
    } else {
        args.extend(paths.iter().map(String::as_str));
    }
    run(host, &root, &args)?;
    Ok(())
}

#[derive(Serialize)]
pub struct GitIdentity {
    pub name: String,
    pub email: String,
}
pub fn identity(host: &mut dyn HostServices, root: &str) -> GitIdentity {
    let root = Path::new(root);
    GitIdentity {
        name: output(host, root, &["config", "user.name"]).unwrap_or_default(),
        email: output(host, root, &["config", "user.email"]).unwrap_or_default(),
    }
}

pub fn commit(
    host: &mut dyn HostServices,
    root: &str,
    message: &str,
    name: &str,
    email: &str,
) -> Result<String, String> {
    let root = repository(host, root)?;
    let message = message.trim();
    if message.is_empty() {
        return Err("请填写提交说明".into());
    }
    if name.trim().is_empty() || email.trim().is_empty() {
        return Err("请填写提交者姓名和邮箱".into());
    }
    run(
        host,
        &root,
        &["config", "--local", "user.name", name.trim()],
    )?;
    run(
        host,
        &root,
        &["config", "--local", "user.email", email.trim()],
    )?;
    run(host, &root, &["commit", "-m", message])?;
    run(host, &root, &["rev-parse", "--short", "HEAD"])
}

#[cfg(test)]
mod tests {
    use super::*;
    use fiex_plugin_sdk::RpcError;
    use serde_json::{json, Value};
    use std::process::Command;

    struct LocalHost;
    impl HostServices for LocalHost {
        fn invoke(&mut self, method: &str, params: Value) -> Result<Value, RpcError> {
            if method != "host.exec" {
                return Err(RpcError::invalid_params("unexpected method"));
            }
            let out = Command::new(params["command"].as_str().unwrap_or("git"))
                .args(
                    params["args"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str),
                )
                .current_dir(params["cwd"].as_str().unwrap_or("."))
                .output()
                .map_err(|error| RpcError::internal(error.to_string()))?;
            use base64::{engine::general_purpose::STANDARD, Engine};
            Ok(json!({
                "code": out.status.code(),
                "stdout": String::from_utf8_lossy(&out.stdout),
                "stdoutBase64": STANDARD.encode(&out.stdout),
                "stderr": String::from_utf8_lossy(&out.stderr)
            }))
        }
    }

    #[test]
    fn initialize_stage_unstage_first_commit_and_modify() {
        let temp = tempfile::tempdir().unwrap();
        let mut host = LocalHost;
        let root = temp.path().to_str().unwrap();
        std::fs::write(temp.path().join("notes.txt"), "first").unwrap();
        let initialized = init(&mut host, root).unwrap();
        assert_eq!(initialized.branch, "main");
        stage(&mut host, root, &[], true).unwrap();
        unstage(&mut host, root, &[], true).unwrap();
        assert!(temp.path().join("notes.txt").exists());
        assert!(status(&mut host, root)
            .unwrap()
            .entries
            .values()
            .any(|v| v == "??"));
        stage(&mut host, root, &[], true).unwrap();
        assert!(!commit(
            &mut host,
            root,
            "First commit",
            "Fiex Test",
            "fiex@example.com"
        )
        .unwrap()
        .is_empty());
        assert_eq!(
            output(&mut host, temp.path(), &["config", "--local", "user.name"]).unwrap(),
            "Fiex Test"
        );
        assert!(status(&mut host, root).unwrap().entries.is_empty());
        std::fs::write(temp.path().join("notes.txt"), "second").unwrap();
        stage(&mut host, root, &[], true).unwrap();
        unstage(&mut host, root, &[], true).unwrap();
        assert!(status(&mut host, root)
            .unwrap()
            .entries
            .values()
            .any(|v| v == " M"));
    }

    #[test]
    fn paths_are_literal_and_cannot_escape_repository() {
        let temp = tempfile::tempdir().unwrap();
        let mut host = LocalHost;
        let root = temp.path().to_str().unwrap();
        init(&mut host, root).unwrap();
        for name in ["[draft].txt", "draft.txt", "-option.txt"] {
            std::fs::write(temp.path().join(name), "x").unwrap();
        }
        stage(
            &mut host,
            root,
            &[temp
                .path()
                .join("[draft].txt")
                .to_string_lossy()
                .into_owned()],
            false,
        )
        .unwrap();
        let state = status(&mut host, root).unwrap();
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
            &mut host,
            root,
            &[temp
                .path()
                .join("../outside")
                .to_string_lossy()
                .into_owned()],
            false,
        )
        .is_err());
        assert!(commit(&mut host, root, " ", "Test", "test@example.com").is_err());
    }
}
