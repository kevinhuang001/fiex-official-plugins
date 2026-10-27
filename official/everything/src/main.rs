//! Optional Windows search adapter. The authorized ES command runs in Host;
//! the plugin never contacts Everything's native IPC endpoint itself.
use fiex_plugin_sdk::{self as sdk, RpcError};
use serde_json::{json, Value};
#[cfg(windows)]
fn query(p: &Value, host: &sdk::RequestContext) -> Result<Value, RpcError> {
    let query = p.get("query").and_then(Value::as_str).unwrap_or("").trim();
    // Core now sends the search root as an opaque ref. The ES adapter must
    // not receive host paths; `process_run` resolves command args that carry
    // the SDK's `fiex-ref:` prefix inside the operation scope.
    let root = p.get("root").and_then(|value| {
        serde_json::from_value::<sdk::ResourceRef>(
            value.get("ref").cloned().unwrap_or(value.clone()),
        )
        .ok()
    });
    let hidden = p.get("hidden").and_then(Value::as_bool).unwrap_or(false);
    let limit = p
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(200)
        .clamp(1, 500) as usize;
    let mut entries = Vec::new();
    for (filter, is_dir) in [("/ad", true), ("/a-d", false)] {
        let mut args = vec!["-n".into(), (limit + 1).to_string(), filter.into()];
        if let Some(root) = &root {
            args.extend(["-path".into(), root.command_arg()]);
        }
        if !hidden {
            args.push("/a-h".into());
        }
        args.push(query.into());
        let output = host.process_run("es", &args, None)?;
        if output.code != Some(0) || output.truncated {
            return Err(RpcError::internal(format!(
                "Everything ES 查询失败，请确认 Everything 正在运行且 es.exe 位于 PATH：{}",
                output.stderr
            )));
        }
        for line in output.stdout.lines() {
            let path = line.trim_end_matches('\r');
            if path.is_empty() {
                continue;
            }
            let name = path.rsplit(['\\', '/']).next().unwrap_or(path);
            if !hidden && name.starts_with('.') {
                continue;
            }
            let extension = std::path::Path::new(name)
                .extension()
                .unwrap_or_default()
                .to_string_lossy();
            entries.push(json!({"path":path,"name":name,"isDir":is_dir,"isSymlink":false,"size":0,"modified":0,"created":0,"extension":extension,"mime":if is_dir{"inode/directory".into()}else{mime_guess::from_path(name).first_or_octet_stream().to_string()},"readonly":false,"linkTarget":null,"tags":[],"sync":null}));
        }
    }
    entries.sort_by_key(|v| v["path"].as_str().unwrap_or("").to_lowercase());
    let truncated = entries.len() > limit;
    entries.truncate(limit);
    Ok(json!({"entries":entries,"truncated":truncated}))
}
#[cfg(not(windows))]
fn query(_p: &Value, _host: &sdk::RequestContext) -> Result<Value, RpcError> {
    Ok(json!({"entries":[],"truncated":false,"warning":"Everything 搜索只在 Windows 上可用"}))
}
fn dispatch(ctx: sdk::RequestContext) -> Result<Value, RpcError> {
    let method = ctx.method();
    match method {
        "search.query" => query(ctx.params(), &ctx),
        "search.status" => Ok(
            json!({"available":cfg!(windows),"provider":"everything","indexOwner":"Everything","requires":"Everything and ES command in PATH"}),
        ),
        "search.rebuild" => Err(RpcError::invalid_params("索引由 Everything 管理")),
        _ => Err(RpcError::method_not_found(method)),
    }
}
fn main() {
    if let Err(e) = sdk::serve("org.fiex.everything", dispatch) {
        eprintln!("Everything IPC failed: {e}");
        std::process::exit(1);
    }
}
