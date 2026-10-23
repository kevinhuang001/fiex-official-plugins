use crate::git::repository;
use git2::{
    build::CheckoutBuilder, AutotagOption, Cred, CredentialType, FetchOptions, RemoteCallbacks,
    Repository,
};
use serde::Serialize;

fn remote_host(url: &str) -> Option<String> {
    let value = url.trim();
    if value.starts_with("https://") || value.starts_with("http://") || value.starts_with("ssh://")
    {
        return url::Url::parse(value)
            .ok()
            .and_then(|parsed| parsed.host_str().map(str::to_ascii_lowercase));
    }
    if let Some((user_host, _)) = value.split_once(':') {
        if let Some((_, host)) = user_host.rsplit_once('@') {
            if !host.is_empty() {
                return Some(host.to_ascii_lowercase());
            }
        }
    }
    None
}

fn ensure_domain(url: &str, domains: &[String]) -> Result<(), String> {
    let Some(host) = remote_host(url) else {
        return Ok(());
    };
    if domains
        .iter()
        .any(|domain| domain.eq_ignore_ascii_case(&host))
    {
        Ok(())
    } else {
        Err(format!("网络域名未授权：{host}"))
    }
}

#[derive(Serialize)]
pub struct Remote {
    pub name: String,
    pub url: String,
}
#[derive(Serialize)]
pub struct RemoteStatus {
    pub remotes: Vec<Remote>,
    pub branch: String,
    pub upstream: String,
    pub ahead: usize,
    pub behind: usize,
}

pub fn status(root: &str) -> Result<RemoteStatus, String> {
    let repo = repository(root)?;
    let mut remotes = Vec::new();
    for name in repo.remotes().map_err(|error| error.to_string())?.iter() {
        let Ok(Some(name)) = name else { continue };
        let url = repo
            .find_remote(name)
            .ok()
            .and_then(|remote| remote.url().ok().map(str::to_owned))
            .unwrap_or_default();
        remotes.push(Remote {
            name: name.to_owned(),
            url,
        });
    }
    let head = repo.head().ok();
    let branch = head
        .as_ref()
        .and_then(|head| head.shorthand().ok().map(str::to_owned))
        .unwrap_or_default();
    let upstream_ref = head
        .as_ref()
        .and_then(|head| head.name().ok())
        .and_then(|name| repo.branch_upstream_name(name).ok())
        .and_then(|name| name.as_str().ok().map(str::to_owned));
    let upstream = upstream_ref
        .as_deref()
        .map(|name| name.trim_start_matches("refs/remotes/").to_owned())
        .unwrap_or_default();
    let (ahead, behind) = match (head.as_ref(), upstream_ref.as_deref()) {
        (Some(head), Some(upstream_ref)) => {
            let local = head.peel_to_commit().ok();
            let upstream = repo.refname_to_id(upstream_ref).ok();
            match (local, upstream) {
                (Some(local), Some(upstream)) => repo
                    .graph_ahead_behind(local.id(), upstream)
                    .unwrap_or((0, 0)),
                _ => (0, 0),
            }
        }
        _ => (0, 0),
    };
    Ok(RemoteStatus {
        remotes,
        branch,
        upstream,
        ahead,
        behind,
    })
}

pub fn save(root: &str, name: &str, url: &str, domains: &[String]) -> Result<(), String> {
    if name.is_empty()
        || name.starts_with('-')
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
    {
        return Err("请输入有效的远程名称".into());
    }
    let url = url.trim();
    if url.is_empty() || url.starts_with('-') || url.contains(['\n', '\r', '\0']) {
        return Err("请输入有效的仓库地址".into());
    }
    ensure_domain(url, domains)?;
    let repo = repository(root)?;
    if repo.find_remote(name).is_ok() {
        repo.remote_set_url(name, url)
            .map_err(|error| error.to_string())?;
    } else {
        repo.remote(name, url).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn remote<'a>(repo: &'a Repository, name: &'a str) -> Result<&'a str, String> {
    repo.find_remote(name)
        .map_err(|_| "请选择已配置的远程仓库".to_string())?;
    Ok(name)
}

pub fn remove(root: &str, name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.starts_with('-')
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
    {
        return Err("请输入有效的远程名称".into());
    }
    let repo = repository(root)?;
    repo.remote_delete(name)
        .map_err(|_| "远程仓库不存在".into())
}

fn callbacks() -> RemoteCallbacks<'static> {
    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(|url, username, allowed| {
        if allowed.contains(CredentialType::SSH_KEY) {
            if let Some(username) = username {
                if let Ok(cred) = Cred::ssh_key_from_agent(username) {
                    return Ok(cred);
                }
            }
        }
        if allowed.contains(CredentialType::USER_PASS_PLAINTEXT) {
            if let Ok(parsed) = url::Url::parse(url) {
                let user = parsed.username();
                if !user.is_empty() {
                    return Cred::userpass_plaintext(user, parsed.password().unwrap_or(""));
                }
            }
        }
        Cred::default()
    });
    callbacks
}

fn fast_forward(repo: &Repository, remote_branch: &str) -> Result<(), String> {
    let fetch_head = repo
        .find_reference("FETCH_HEAD")
        .map_err(|error| error.to_string())?;
    let target = fetch_head
        .peel_to_commit()
        .map_err(|error| error.to_string())?;
    let head = repo
        .head()
        .and_then(|head| head.peel_to_commit())
        .map_err(|error| error.to_string())?;
    if target.id() != head.id()
        && !repo
            .graph_descendant_of(target.id(), head.id())
            .map_err(|error| error.to_string())?
    {
        return Err("本地分支与远程已分叉，无法快进".into());
    }
    let mut checkout = CheckoutBuilder::new();
    checkout.safe();
    repo.checkout_tree(target.as_object(), Some(&mut checkout))
        .map_err(|error| error.to_string())?;
    let reference = format!("refs/heads/{remote_branch}");
    repo.reference(&reference, target.id(), true, "fiex pull")
        .map_err(|error| error.to_string())?;
    repo.set_head(&reference)
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn sync(
    root: &str,
    name: &str,
    push: bool,
    branch: Option<&str>,
    domains: &[String],
) -> Result<String, String> {
    let repo = repository(root)?;
    let name = remote(&repo, name)?;
    let url = repo
        .find_remote(name)
        .ok()
        .and_then(|remote| remote.url().ok().map(str::to_owned))
        .ok_or_else(|| "远程仓库不存在".to_string())?;
    ensure_domain(&url, domains)?;
    let branch = match branch.map(str::trim).filter(|value| !value.is_empty()) {
        Some(chosen) => chosen.to_string(),
        None => repo
            .head()
            .and_then(|head| head.shorthand().map(str::to_owned))
            .map_err(|_| "请先返回本地分支".to_string())?,
    };
    if push {
        let mut remote = repo.find_remote(name).map_err(|e| e.to_string())?;
        let callbacks = callbacks();
        let mut options = git2::PushOptions::new();
        options.remote_callbacks(callbacks);
        let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");
        remote
            .push(&[refspec.as_str()], Some(&mut options))
            .map_err(|error| error.to_string())?;
        if let Ok(mut local) = repo.find_branch(&branch, git2::BranchType::Local) {
            let _ = local.set_upstream(Some(&format!("{name}/{branch}")));
        }
        Ok(String::new())
    } else {
        let mut remote = repo.find_remote(name).map_err(|e| e.to_string())?;
        let callbacks = callbacks();
        let mut options = FetchOptions::new();
        options.remote_callbacks(callbacks);
        options.download_tags(AutotagOption::All);
        remote
            .fetch(&[branch.as_str()], Some(&mut options), None)
            .map_err(|error| error.to_string())?;
        fast_forward(&repo, &branch)?;
        Ok(String::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[test]
    fn push_pull_and_divergence_preserve_local_history() {
        let t = tempfile::tempdir().unwrap();
        let bare = t.path().join("remote.git");
        let mut options = git2::RepositoryInitOptions::new();
        options.bare(true).initial_head("main");
        Repository::init_opts(&bare, &options).unwrap();
        let a = t.path().join("a");
        fs::create_dir(&a).unwrap();
        let root = a.to_str().unwrap();
        crate::git::init(root).unwrap();
        fs::write(a.join("a.txt"), "a").unwrap();
        crate::git::stage(root, &[], true).unwrap();
        crate::git::commit(root, "first", "Test", "test@example.com").unwrap();
        save(root, "origin", bare.to_str().unwrap(), &[]).unwrap();
        sync(root, "origin", true, None, &[]).unwrap();
        assert!(status(root).unwrap().upstream.contains("origin/main"));

        let b = t.path().join("b");
        git2::build::RepoBuilder::new()
            .clone(bare.to_str().unwrap(), &b)
            .unwrap();
        fs::write(b.join("b.txt"), "remote").unwrap();
        crate::git::stage(b.to_str().unwrap(), &[], true).unwrap();
        crate::git::commit(b.to_str().unwrap(), "remote", "Test", "test@example.com").unwrap();
        sync(b.to_str().unwrap(), "origin", true, None, &[]).unwrap();
        sync(root, "origin", false, None, &[]).unwrap();
        assert_eq!(fs::read_to_string(a.join("b.txt")).unwrap(), "remote");
        fs::write(a.join("a.txt"), "local").unwrap();
        crate::git::stage(root, &[], true).unwrap();
        let before = crate::git::commit(root, "local", "Test", "test@example.com").unwrap();
        fs::write(b.join("c.txt"), "remote2").unwrap();
        crate::git::stage(b.to_str().unwrap(), &[], true).unwrap();
        crate::git::commit(b.to_str().unwrap(), "remote2", "Test", "test@example.com").unwrap();
        sync(b.to_str().unwrap(), "origin", true, None, &[]).unwrap();
        assert!(sync(root, "origin", false, None, &[]).is_err());
        assert_eq!(crate::git::status(root).unwrap().entries.len(), 0);
        assert!(!a.join("c.txt").exists());
        assert!(!before.is_empty());
        assert!(save(root, "--bad", "x", &[]).is_err());
        assert!(sync(root, "missing", true, None, &[]).is_err());
    }
}
