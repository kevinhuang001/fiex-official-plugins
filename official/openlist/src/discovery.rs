use crate::{
    client::{self, Session},
    fs,
};
use fiex_plugin_sdk::{self as sdk, filesystem::FsOperation, RpcError};
use serde_json::{json, Value};

pub fn discover(
    ctx: &dyn sdk::ProviderInvocation,
    session: &Session,
) -> Result<Vec<FsOperation>, RpcError> {
    let request = ctx
        .input()?
        .vfs_request()
        .ok_or_else(|| RpcError::invalid_params("missing VFS request"))?;
    let connection = request
        .connection_value()
        .map_err(RpcError::invalid_params)?;
    let cfg = client::config(connection.as_ref())?;
    let node = request
        .node_id
        .as_deref()
        .or(request.parent_id.as_deref())
        .unwrap_or("");
    fs::safe_relative(node)?;
    // /api/fs/list reports the directory's meta write gate. For a file,
    // mutations operate in its parent; mkdir already supplies parentId.
    let directory_target = ctx.method() == "vfs.list"
        || request.parent_id.is_some()
        || (!node.is_empty()
            && fs::object(ctx, &cfg, session, node)?
                .pointer("/data/is_dir")
                .and_then(Value::as_bool)
                == Some(true));
    let directory = if directory_target {
        node
    } else if node.is_empty() {
        ""
    } else {
        fs::split_path(node).0
    };
    let listing = client::authorized(
        ctx,
        &cfg,
        session,
        "/api/fs/list",
        &json!({
            "path":fs::api_path(directory),"password":"","page":1,"per_page":1,"refresh":false
        }),
    )?;
    let user = client::current_user(ctx, &cfg, session)?;
    Ok(operations(&user, &listing["data"]))
}
fn operations(user: &Value, directory: &Value) -> Vec<FsOperation> {
    let mut ops = vec![FsOperation::List, FsOperation::Stat, FsOperation::Read];
    if directory["write"].as_bool() != Some(true) {
        return ops;
    }
    let admin = user["role"].as_i64() == Some(2);
    let permission = user["permission"].as_u64().unwrap_or(0);
    let has = |bit| admin || permission & (1u64 << bit) != 0u64;
    if has(3) || directory["write_content_bypass"].as_bool() == Some(true) {
        ops.extend([FsOperation::Write, FsOperation::Mkdir]);
    }
    // rename also implements cross-directory moves; require both bits.
    if has(4) && has(5) {
        ops.push(FsOperation::Rename);
    }
    if has(7) {
        ops.push(FsOperation::Remove);
    }
    ops
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn user_bits_and_path_gate_are_intersected() {
        let admin = json!({"role":2});
        assert!(!operations(&admin, &json!({"write":false})).contains(&FsOperation::Write));
        assert!(operations(&admin, &json!({"write":true})).contains(&FsOperation::Remove));
        let uploader = json!({"permission":8});
        let ops = operations(&uploader, &json!({"write":true}));
        assert!(ops.contains(&FsOperation::Mkdir));
        assert!(!ops.contains(&FsOperation::Rename));
        assert!(!ops.contains(&FsOperation::Remove));
    }
}
