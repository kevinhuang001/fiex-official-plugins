use super::connection::*;
use super::*;
pub(super) fn directory_entries(
    tcp: &sdk::TcpClient,
    cfg: &StoredConnection,
    path: &str,
) -> Result<Vec<FsEntry>, RpcError> {
    let (connection, session, tree) = connect(tcp, cfg).map_err(fail)?;
    let result = (|| {
        let dir = tree
            .create(
                &smb_path(cfg, path),
                &smb::FileCreateArgs::make_open_existing(
                    smb::DirAccessMask::new()
                        .with_list_directory(true)
                        .with_synchronize(true)
                        .into(),
                ),
            )
            .map_err(fail)?
            .unwrap_dir();
        let mut entries = Vec::new();
        for item in dir
            .query::<smb::FileFullDirectoryInformation>("*")
            .map_err(fail)?
        {
            let item = item.map_err(fail)?;
            let name = item.file_name.to_string();
            if name == "." || name == ".." {
                continue;
            }
            if entries.len() >= 100000 {
                return Err(fail("remote directory too large"));
            }
            let directory = item.file_attributes.directory();
            let node = if path.is_empty() {
                name.clone()
            } else {
                format!("{path}/{name}")
            };
            let mut entry = if directory {
                FsEntry::directory(node, name)
            } else {
                FsEntry::file(node, name)
            };
            entry.size = Some(item.end_of_file);
            entry.modified = Some(0);
            entry.revision = Some(format!(
                "{:?}:{:?}:{}",
                item.last_write_time, item.change_time, item.end_of_file
            ));
            entries.push(entry);
        }
        let _ = dir.close();
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(entries)
    })();
    close(connection, session, tree);
    result
}

pub(super) fn list(request: &FsRequest<'_>) -> Result<FsListing, RpcError> {
    let cfg = connection(request)?;
    let path = request.node_id().unwrap_or("").to_owned();
    safe_relative(&path)?;
    safe_relative(&cfg.base)?;
    let entries = directory_entries(&request.ctx().tcp(), &cfg, &path)?;
    let start = request
        .cursor()
        .unwrap_or("0")
        .parse::<usize>()
        .map_err(|e| fail(e.to_string()))?;
    if start > entries.len() {
        return Err(fail("invalid pagination cursor"));
    }
    let limit = request.limit().clamp(1, 500);
    let end = start.saturating_add(limit).min(entries.len());
    let next_cursor = (end < entries.len()).then(|| end.to_string());
    Ok(FsListing {
        entries: entries[start..end].to_vec(),
        next_cursor,
        warnings: Vec::new(),
        truncated: false,
    })
}

pub(super) fn stat_path(
    tcp: &sdk::TcpClient,
    cfg: &StoredConnection,
    node: &str,
) -> Result<FsEntry, RpcError> {
    let (connection, session, tree) = connect(tcp, cfg).map_err(fail)?;
    let result = (|| {
        let target = smb_path(cfg, node);
        let resource = tree
            .create(
                &target,
                &smb::FileCreateArgs::make_open_existing(
                    smb::FileAccessMask::new().with_generic_read(true),
                ),
            )
            .or_else(|_| {
                tree.create(
                    &target,
                    &smb::FileCreateArgs::make_open_existing(
                        smb::DirAccessMask::new()
                            .with_list_directory(true)
                            .with_synchronize(true)
                            .into(),
                    ),
                )
            })
            .map_err(fail)?;
        if resource.is_dir() {
            let dir = resource.unwrap_dir();
            let revision = directory_revision(&dir)?;
            let _ = dir.close();
            return Ok(FsEntry::directory(node.to_string(), display_name(node))
                .with_size(0)
                .with_revision(revision));
        }
        let file = resource.unwrap_file();
        let size = file
            .query_info::<smb::FileStandardInformation>()
            .map_err(fail)?
            .end_of_file;
        let revision = revision(&file)?;
        let _ = file.close();
        Ok(FsEntry::file(node.to_string(), display_name(node))
            .with_size(size)
            .with_revision(revision))
    })();
    close(connection, session, tree);
    result
}

pub(super) fn stat(request: &FsRequest<'_>) -> Result<FsEntry, RpcError> {
    let cfg = connection(request)?;
    let node = request.node_id().unwrap_or("").to_owned();
    safe_relative(&node)?;
    safe_relative(&cfg.base)?;
    // The mount root is the configured base directory.
    if node.is_empty() {
        return Ok(FsEntry::directory("", "root").with_size(0));
    }
    stat_path(&request.ctx().tcp(), &cfg, &node)
}
