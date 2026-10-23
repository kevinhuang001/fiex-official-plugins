//! Small Git operations delegated to the host process runner.
//!
//! Core chooses the working directory (the focused explorer directory); this
//! module only supplies command arguments and parses the returned output.
use fiex_plugin_sdk::HostServices;
use serde_json::{json, Value};
use std::collections::HashMap;

fn run(host: &mut dyn HostServices, args: &[&str]) -> Result<String, String> {
    let args = args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
    let result = host
        .process_run("git", &args, None)
        .map_err(|error| error.to_string())?;
    if result.code != Some(0) {
        return Err(if result.stderr.trim().is_empty() {
            format!("git exited with {:?}", result.code)
        } else {
            result.stderr.trim().to_owned()
        });
    }
    Ok(result.stdout)
}

pub fn status(root: &str, host: &mut dyn HostServices) -> Option<Value> {
    let branch = run(host, &["rev-parse", "--abbrev-ref", "HEAD"])
        .or_else(|_| run(host, &["symbolic-ref", "--short", "HEAD"]))
        .ok()?;
    let output = run(
        host,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )
    .ok()?;
    let mut entries = HashMap::new();
    for item in output.split('\0').filter(|item| !item.is_empty()) {
        let mut chars = item.chars();
        let first = chars.next().unwrap_or(' ');
        let second = chars.next().unwrap_or(' ');
        let code = if first == '?' {
            "??".to_string()
        } else {
            format!("{first}{second}")
        };
        let path = item.chars().skip(3).collect::<String>();
        if path.is_empty() {
            continue;
        }
        entries.insert(
            std::path::Path::new(root)
                .join(&path)
                .to_string_lossy()
                .into_owned(),
            code,
        );
    }
    Some(json!({
        "root": root,
        "branch": branch.trim(),
        "entries": entries
    }))
}

pub fn init(root: &str, host: &mut dyn HostServices) -> Result<Value, String> {
    run(host, &["init", "-b", "main"])?;
    status(root, host).ok_or("仓库已创建，但无法读取状态".into())
}

pub fn refs(_root: &str, host: &mut dyn HostServices) -> Result<Value, String> {
    let current = run(host, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    let head = run(host, &["rev-parse", "--short=7", "HEAD"]).unwrap_or_default();
    let branches = run(host, &["branch", "--format=%(refname:short)"])?
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let tags = run(host, &["tag", "--list"])?
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    Ok(json!({"head": head.trim(), "current": current.trim(), "branches": branches, "tags": tags}))
}

pub fn identity(_root: &str, host: &mut dyn HostServices) -> Result<Value, String> {
    let name = run(host, &["config", "user.name"]).unwrap_or_default();
    let email = run(host, &["config", "user.email"]).unwrap_or_default();
    Ok(json!({"name": name.trim(), "email": email.trim()}))
}

pub fn stage(
    _root: &str,
    paths: &[String],
    all: bool,
    host: &mut dyn HostServices,
) -> Result<Value, String> {
    if all {
        run(host, &["add", "-A"])?;
    } else if !paths.is_empty() {
        let mut args = vec!["add", "--"];
        args.extend(paths.iter().map(String::as_str));
        run(host, &args)?;
    }
    Ok(Value::Null)
}

pub fn unstage(
    _root: &str,
    paths: &[String],
    all: bool,
    host: &mut dyn HostServices,
) -> Result<Value, String> {
    if all {
        run(host, &["reset"])?;
    } else if !paths.is_empty() {
        let mut args = vec!["reset", "--"];
        args.extend(paths.iter().map(String::as_str));
        run(host, &args)?;
    }
    Ok(Value::Null)
}

pub fn commit(
    _root: &str,
    message: &str,
    name: &str,
    email: &str,
    host: &mut dyn HostServices,
) -> Result<Value, String> {
    let name = format!("user.name={name}");
    let email = format!("user.email={email}");
    run(
        host,
        &[
            "-c",
            &name,
            "-c",
            &email,
            "commit",
            "--no-verify",
            "-m",
            message,
        ],
    )?;
    let hash = run(host, &["rev-parse", "--short=7", "HEAD"])?;
    Ok(json!(hash.trim()))
}
