//! Read the access actually granted by an authenticated MAXIMUM_ALLOWED open.
//! Never use the library's optimistic maximal-access-context fallback.
use crate::{close, connect, fail, safe_relative, smb_path, StoredConnection};
use fiex_plugin_sdk::{self as sdk, filesystem::FsOperation, RpcError};

pub fn discover(ctx: &sdk::RequestContext) -> Result<Vec<FsOperation>, RpcError> {
    let cfg: StoredConnection = serde_json::from_value(ctx.params()["connection"].clone())
        .map_err(|e| RpcError::invalid_params(e.to_string()))?;
    let node = ctx.params()["nodeId"]
        .as_str()
        .or_else(|| ctx.params()["parentId"].as_str())
        .unwrap_or("");
    safe_relative(node)?;
    safe_relative(&cfg.base)?;
    let (connection, session, tree) = connect(&ctx.tcp(), &cfg).map_err(fail)?;
    let result = (|| {
        let (mask, directory) = access(&tree, &cfg, node)?;
        let parent_mask = if node.is_empty() {
            0
        } else {
            let parent = node
                .rsplit_once('/')
                .map(|(parent, _)| parent)
                .unwrap_or("");
            access(&tree, &cfg, parent)?.0
        };
        Ok(operations(mask, parent_mask, directory, node.is_empty()))
    })();
    close(connection, session, tree);
    result
}
fn access(tree: &smb::Tree, cfg: &StoredConnection, node: &str) -> Result<(u32, bool), RpcError> {
    let resource = tree
        .create(
            &smb_path(cfg, node),
            &smb::FileCreateArgs::make_open_existing(
                smb::FileAccessMask::new().with_maximum_allowed(true),
            ),
        )
        .map_err(fail)?;
    let directory = resource.is_dir();
    let info = if directory {
        let dir = resource.unwrap_dir();
        let info = dir.query_info::<smb::FileAccessInformation>();
        let _ = dir.close();
        info
    } else {
        let file = resource.unwrap_file();
        let info = file.query_info::<smb::FileAccessInformation>();
        let _ = file.close();
        info
    }
    .map_err(fail)?;
    Ok((
        u32::from_le_bytes(info.access_flags.into_bytes()),
        directory,
    ))
}
fn operations(mask: u32, parent: u32, directory: bool, root: bool) -> Vec<FsOperation> {
    let mut ops = vec![FsOperation::Stat];
    if mask & 1 != 0 {
        ops.push(FsOperation::Read);
        if directory {
            ops.push(FsOperation::List);
        }
    }
    // FILE_WRITE_DATA / FILE_ADD_FILE and FILE_ADD_SUBDIRECTORY.
    if mask & 2 != 0 {
        ops.push(FsOperation::Write);
    }
    if directory && mask & 4 != 0 {
        ops.push(FsOperation::Mkdir);
    }
    let deletable = mask & 0x10000 != 0 || parent & 0x40 != 0;
    if !root && deletable {
        ops.push(FsOperation::Remove);
        if parent & (if directory { 4 } else { 2 }) != 0 {
            ops.push(FsOperation::Rename);
        }
    }
    ops
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn read_only_share_and_parent_delete_rights() {
        assert_eq!(operations(1, 0, false, false), vec![FsOperation::Stat, FsOperation::Read]);
        assert!(!operations(0x10003, 0, false, false).contains(&FsOperation::Rename));
        assert!(operations(1, 0x42, false, false).contains(&FsOperation::Rename));
        assert!(!operations(u32::MAX, u32::MAX, true, true).contains(&FsOperation::Remove));
    }
}
