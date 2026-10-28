//! Small Git operations delegated to the host process runner.
//!
//! Core chooses the working directory (the focused explorer directory); this
//! module only supplies command arguments and parses the returned output.
use fiex_plugin_sdk as sdk;
use serde_json::{json, Value};
use std::collections::HashMap;

pub fn run(
    host: &sdk::RequestContext,
    root: &sdk::ResourceRef,
    args: &[&str],
) -> Result<String, String> {
    let args = args
        .iter()
        .map(|arg| (*arg).to_string())
        .collect::<Vec<_>>();
    let result = host
        .process_run_at(root, "git", &args, None)
        .map_err(|error| error.to_string())?;
    if result.truncated {
        return Err(crate::i18n::t("命令输出超过限制，请缩小查询范围").into());
    }
    if result.code != Some(0) {
        return Err(if result.stderr.trim().is_empty() {
            format!("git exited with {:?}", result.code)
        } else {
            result.stderr.trim().to_owned()
        });
    }
    Ok(result.stdout)
}

pub fn status(root: &sdk::ResourceRef, host: &sdk::RequestContext) -> Option<Value> {
    let branch = run(host, root, &["rev-parse", "--abbrev-ref", "HEAD"])
        .or_else(|_| run(host, root, &["symbolic-ref", "--short", "HEAD"]))
        .ok()?;
    let output = run(
        host,
        root,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )
    .ok()?;
    let mut entries = HashMap::new();
    let mut records = output.split('\0').filter(|item| !item.is_empty());
    while let Some(item) = records.next() {
        if item.len() < 3 {
            continue;
        }
        let code = &item[..2];
        let path = &item[3..];
        entries.insert(path.to_owned(), code.to_owned());
        if code.contains(['R', 'C']) {
            records.next();
        }
    }
    Some(json!({
        "root": root,
        "branch": branch.trim(),
        "entries": entries
    }))
}

pub fn init(root: &sdk::ResourceRef, host: &sdk::RequestContext) -> Result<Value, String> {
    run(host, root, &["init", "-b", "main"])?;
    status(root, host).ok_or(crate::i18n::t("仓库已创建，但无法读取状态").into())
}

pub fn refs(root: &sdk::ResourceRef, host: &sdk::RequestContext) -> Result<Value, String> {
    let current = run(host, root, &["rev-parse", "--abbrev-ref", "HEAD"])
        .or_else(|_| run(host, root, &["symbolic-ref", "--short", "HEAD"]))?;
    let head = run(host, root, &["rev-parse", "--short=7", "HEAD"]).unwrap_or_default();
    let branches = run(host, root, &["branch", "--format=%(refname:short)"])?
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let tags = run(host, root, &["tag", "--list"])?
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    Ok(json!({"head": head.trim(), "current": current.trim(), "branches": branches, "tags": tags}))
}

pub fn identity(root: &sdk::ResourceRef, host: &sdk::RequestContext) -> Result<Value, String> {
    let name = run(host, root, &["config", "user.name"]).unwrap_or_default();
    let email = run(host, root, &["config", "user.email"]).unwrap_or_default();
    Ok(json!({"name": name.trim(), "email": email.trim()}))
}

pub fn stage(
    root: &sdk::ResourceRef,
    paths: &[String],
    all: bool,
    host: &sdk::RequestContext,
) -> Result<Value, String> {
    if all {
        run(host, root, &["add", "-A"])?;
    } else if !paths.is_empty() {
        let mut args = vec!["add", "--"];
        args.extend(paths.iter().map(String::as_str));
        run(host, root, &args)?;
    }
    Ok(Value::Null)
}

pub fn unstage(
    root: &sdk::ResourceRef,
    paths: &[String],
    all: bool,
    host: &sdk::RequestContext,
) -> Result<Value, String> {
    let committed = run(host, root, &["rev-parse", "--verify", "HEAD"]).is_ok();
    let mut args = if committed {
        vec!["reset", "--"]
    } else {
        vec!["rm", "--cached", "-r", "--ignore-unmatch", "--"]
    };
    if all {
        args.push(".");
    } else {
        args.extend(paths.iter().map(String::as_str));
    }
    if all || !paths.is_empty() {
        run(host, root, &args)?;
    }
    Ok(Value::Null)
}

pub fn commit(
    root: &sdk::ResourceRef,
    message: &str,
    name: &str,
    email: &str,
    host: &sdk::RequestContext,
) -> Result<Value, String> {
    let name = format!("user.name={name}");
    let email = format!("user.email={email}");
    run(
        host,
        root,
        &["-c", &name, "-c", &email, "commit", "-m", message],
    )?;
    let hash = run(host, root, &["rev-parse", "--short=7", "HEAD"])?;
    Ok(json!(hash.trim()))
}
