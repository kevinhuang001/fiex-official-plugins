use super::connection::*;
use super::*;
pub(super) fn mkdir(request: &FsRequest<'_>) -> Result<FsEntry, RpcError> {
    let cfg = connection(request)?;
    let parent = request.parent_id().unwrap_or("");
    safe_relative(parent)?;
    let name = request.name().ok_or_else(|| fail("missing name"))?;
    valid_name(name)?;
    let node = join_path(parent, name);
    safe_relative(&node)?;
    safe_relative(&cfg.base)?;
    let (connection, session, tree) = connect(&request.ctx().tcp(), &cfg).map_err(fail)?;
    let result = (|| {
        let dir = tree
            .create_directory(
                &smb_path(&cfg, &node),
                smb::CreateDisposition::Create,
                smb::FileAccessMask::new().with_generic_all(true),
            )
            .map_err(fail)?
            .unwrap_dir();
        let revision = directory_revision(&dir)?;
        let _ = dir.close();
        Ok(FsEntry::directory(node.clone(), name.to_string())
            .with_size(0)
            .with_revision(revision))
    })();
    close(connection, session, tree);
    result
}

pub(super) fn rename(request: &FsRequest<'_>) -> Result<FsEntry, RpcError> {
    let cfg = connection(request)?;
    let node = request.node_id().ok_or_else(|| fail("missing nodeId"))?;
    if node.is_empty() {
        return Err(fail("cannot rename the mount root"));
    }
    safe_relative(node)?;
    safe_relative(&cfg.base)?;
    let (old_parent, old_name) = split_path(node);
    let new_name = request.name().unwrap_or(old_name);
    valid_name(new_name)?;
    let new_parent = request.parent_id().unwrap_or(old_parent);
    safe_relative(new_parent)?;
    let new_node = join_path(new_parent, new_name);
    safe_relative(&new_node)?;
    let tcp = request.ctx().tcp();
    if new_node == node {
        return stat_path(&tcp, &cfg, &new_node);
    }
    let (connection, session, tree) = connect(&tcp, &cfg).map_err(fail)?;
    let result = (|| {
        let source = mut_open(&tree, &smb_path(&cfg, node))?;
        if let Some(revision) = request.if_revision() {
            if resource_revision(&source)? != revision {
                return Err(fail(crate::i18n::t("同步冲突：远端已修改")));
            }
        }
        let info = smb::FileRenameInformation {
            replace_if_exists: false.into(),
            root_directory: 0,
            file_name: smb_path(&cfg, &new_node).as_str().into(),
        };
        match source {
            smb::Resource::File(file) => {
                file.set_info(info).map_err(fail)?;
                let _ = file.close();
            }
            smb::Resource::Directory(dir) => {
                dir.set_info(info).map_err(fail)?;
                let _ = dir.close();
            }
            _ => return Err(fail("unsupported SMB resource")),
        }
        Ok(())
    })();
    close(connection, session, tree);
    result?;
    stat_path(&tcp, &cfg, &new_node)
}

pub(super) fn remove(request: &FsRequest<'_>) -> Result<(), RpcError> {
    let cfg = connection(request)?;
    let node = request.node_id().ok_or_else(|| fail("missing nodeId"))?;
    if node.is_empty() {
        return Err(fail("cannot remove the mount root"));
    }
    safe_relative(node)?;
    safe_relative(&cfg.base)?;
    let (connection, session, tree) = connect(&request.ctx().tcp(), &cfg).map_err(fail)?;
    let result = (|| {
        let resource = mut_open(&tree, &smb_path(&cfg, node))?;
        if let Some(revision) = request.if_revision() {
            if resource_revision(&resource)? != revision {
                return Err(fail(crate::i18n::t("同步冲突：远端已修改")));
            }
        }
        if resource.is_dir() {
            let dir = resource.unwrap_dir();
            let non_empty = dir
                .query::<smb::FileFullDirectoryInformation>("*")
                .map_err(fail)?
                .filter_map(|item| item.ok())
                .any(|item| {
                    let name = item.file_name.to_string();
                    name != "." && name != ".."
                });
            if non_empty {
                let _ = dir.close();
                return Err(fail(crate::i18n::t("目录不为空，无法删除")));
            }
            dir.set_info(smb::FileDispositionInformation {
                delete_pending: true.into(),
            })
            .map_err(fail)?;
            let _ = dir.close();
        } else {
            let file = resource.unwrap_file();
            file.set_info(smb::FileDispositionInformation {
                delete_pending: true.into(),
            })
            .map_err(fail)?;
            let _ = file.close();
        }
        Ok(())
    })();
    close(connection, session, tree);
    result
}
