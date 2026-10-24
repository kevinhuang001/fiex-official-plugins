use crate::git_host::run;
use fiex_plugin_sdk as sdk;
use serde_json::{json, Value};
fn valid(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.starts_with('-')
        || !name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-/".contains(&c))
    {
        Err("无效的远程名称或分支".into())
    } else {
        Ok(())
    }
}
pub fn status(root: &str, host: &sdk::RequestContext) -> Result<Value, String> {
    let names = run(host, root, &["remote"])?;
    let mut remotes = Vec::new();
    for name in names.lines() {
        let url = run(host, root, &["remote", "get-url", name])?;
        remotes.push(json!({"name":name,"url":url.trim()}));
    }
    let branch = run(host, root, &["symbolic-ref", "--short", "HEAD"]).unwrap_or_default();
    let upstream = run(
        host,
        root,
        &[
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{upstream}",
        ],
    )
    .unwrap_or_default();
    let counts = run(
        host,
        root,
        &["rev-list", "--left-right", "--count", "HEAD...@{upstream}"],
    )
    .unwrap_or_default();
    let mut counts = counts.split_whitespace();
    Ok(
        json!({"remotes":remotes,"branch":branch.trim(),"upstream":upstream.trim(),"ahead":counts.next().and_then(|n|n.parse::<u64>().ok()).unwrap_or(0),"behind":counts.next().and_then(|n|n.parse::<u64>().ok()).unwrap_or(0)}),
    )
}
pub fn save(
    root: &str,
    name: &str,
    url: &str,
    host: &sdk::RequestContext,
) -> Result<Value, String> {
    valid(name)?;
    if url.is_empty() || url.starts_with('-') || url.contains(['\n', '\r', '\0']) {
        return Err("无效的仓库地址".into());
    }
    let exists = run(host, root, &["remote", "get-url", name]).is_ok();
    run(
        host,
        root,
        &["remote", if exists { "set-url" } else { "add" }, name, url],
    )?;
    Ok(Value::Null)
}
pub fn remove(root: &str, name: &str, host: &sdk::RequestContext) -> Result<Value, String> {
    valid(name)?;
    run(host, root, &["remote", "remove", name])?;
    Ok(Value::Null)
}
pub fn sync(
    root: &str,
    name: &str,
    push: bool,
    branch: Option<&str>,
    host: &sdk::RequestContext,
) -> Result<Value, String> {
    valid(name)?;
    if let Some(b) = branch {
        valid(b)?;
    }
    let mut args = if push {
        vec!["push", name]
    } else {
        vec!["pull", "--ff-only", name]
    };
    if let Some(branch) = branch {
        args.push(branch);
    }
    run(host, root, &args)?;
    Ok(Value::Null)
}
