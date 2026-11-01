//! Optional Windows search adapter. The authorized ES command runs in Host;
//! the plugin never contacts Everything's native IPC endpoint itself.
use fiex_plugin_sdk::ProviderInvocationExt;
#[path = "../../../support/i18n.rs"]
mod i18n;
#[cfg(windows)]
use fiex_plugin_sdk::{self as sdk, RpcError};
use serde_json::{json, Value};
#[cfg(windows)]
fn query(
    request: &sdk::SearchQueryRequest,
    host: &dyn sdk::ProviderInvocation,
) -> Result<sdk::SearchResults, RpcError> {
    if request.mode != sdk::SearchMode::Name {
        return Err(RpcError::invalid_params(
            "Everything only supports filename search",
        ));
    }
    let query = request.query.trim();
    // Core now sends the search root as an opaque ref. The ES adapter must
    // not receive host paths; `process_run` resolves command args that carry
    // the SDK's `fiex-ref:` prefix inside the operation scope.
    let root = request
        .root
        .as_ref()
        .ok_or_else(|| RpcError::invalid_params("search root ResourceRef is required"))?;
    let hidden = request.hidden;
    let limit = request.limit.unwrap_or(200).clamp(1, 500);
    let mut entries = Vec::new();
    for (filter, is_dir) in [("/ad", true), ("/a-d", false)] {
        let mut args = vec!["-n".into(), (limit + 1).to_string(), filter.into()];
        // The host resolves the ref inside this operation scope. Never let
        // Everything enumerate the entire host index for a missing root.
        args.extend(["-path".into(), root.command_arg()]);
        if !hidden {
            args.push("/a-h".into());
        }
        args.push(query.into());
        let output = host.process_run("es", &args, None)?;
        if output.code != Some(0) || output.truncated {
            return Err(RpcError::internal(fiex_plugin_sdk::i18n::format(
                crate::i18n::t(
                    "Everything ES 查询失败，请确认 Everything 正在运行且 es.exe 位于 PATH：{0}",
                ),
                &[("0", output.stderr)],
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
            entries.push(sdk::SearchEntry {
                path: path.to_string(),
                name: name.to_string(),
                is_dir,
                is_symlink: false,
                size: 0,
                modified: 0,
                created: 0,
                extension: extension.into_owned(),
                mime: if is_dir {
                    "inode/directory".into()
                } else {
                    mime_guess::from_path(name)
                        .first_or_octet_stream()
                        .to_string()
                },
                readonly: false,
                link_target: None,
                allowed_ops: Vec::new(),
                tags: Vec::new(),
                has_cache: false,
                pinned: false,
                uploaded: false,
                sync: None,
            });
        }
    }
    entries.sort_by_key(|entry| entry.path.to_lowercase());
    let truncated = entries.len() > limit;
    entries.truncate(limit);
    Ok(sdk::SearchResults {
        entries,
        truncated,
        next_cursor: None,
    })
}
#[cfg(not(windows))]
fn query(
    _request: &sdk::SearchQueryRequest,
    _host: &dyn sdk::ProviderInvocation,
) -> Result<sdk::SearchResults, RpcError> {
    Err(RpcError::method_not_found("search.query"))
}
fn dispatch(ctx: sdk::RequestContext) -> Result<Value, RpcError> {
    let method = ctx.method();
    match method {
        "search.status" => Ok(
            json!({"available":cfg!(windows),"provider":"everything","indexOwner":"Everything","requires":"Everything and ES command in PATH"}),
        ),
        "search.rebuild" => Err(RpcError::invalid_params(crate::i18n::t(
            "索引由 Everything 管理",
        ))),
        _ => Err(RpcError::method_not_found(method)),
    }
}
fn main() {
    if let Err(e) = sdk::Providers::new()
        .diagnostics()
        .search(Everything)
        .serve_with("org.fiex.everything", dispatch)
    {
        eprintln!("Everything IPC failed: transport closed ({:?})", e.kind());
        std::process::exit(1);
    }
}

struct Everything;
impl sdk::SearchProvider for Everything {
    fn query(&self, ctx: &dyn sdk::ProviderInvocation) -> Result<sdk::SearchResults, RpcError> {
        let request = ctx
            .input()?
            .search_request()
            .ok_or_else(|| RpcError::invalid_params("missing search request"))?;
        query(request, ctx)
    }
}
