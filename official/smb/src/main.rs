//! SMB protocol implementation. All socket operations are brokered by Core.
mod discovery;
mod sync;
mod transport;
use fiex_plugin_sdk::{
    self as sdk,
    filesystem::{FSProvider, FsEntry, FsListing, FsRead, FsRequest, FsWrite},
    RpcError,
};
use serde::Deserialize;
use serde_json::{json, Value};
use smb::{GetLen, ReadAt};
use std::{io, str::FromStr};

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredConnection {
    host: String,
    share: String,
    #[serde(default)]
    base: String,
    #[serde(default)]
    username: String,
    #[serde(default)]
    password: String,
    port: Option<u16>,
}

/// A real remote file kept open for the lifetime of one `fs.openRead` stream.
/// Reusing the connection, authenticated session, tree and file handle avoids
/// reconnecting and re-authenticating for every read chunk.
struct OpenStream {
    connection: smb::Connection,
    session: smb::Session,
    tree: smb::Tree,
    file: smb::File,
    total: u64,
}

impl Drop for OpenStream {
    fn drop(&mut self) {
        let _ = self.file.close();
        let _ = self.tree.disconnect();
        let _ = self.session.logoff();
        let _ = self.connection.close();
    }
}

/// Standard `Read` over the open SMB handle; the SDK owns offsets and chunks.
struct SmbReader {
    stream: OpenStream,
    offset: u64,
}

impl io::Read for SmbReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let read = self
            .stream
            .file
            .read_at(output, self.offset)
            .map_err(|error| io::Error::other(error.to_string()))?;
        self.offset += read as u64;
        Ok(read)
    }
}

fn fail(e: impl std::fmt::Display) -> RpcError {
    RpcError::internal(e.to_string())
}

fn connect(
    tcp: &sdk::TcpClient,
    cfg: &StoredConnection,
) -> Result<(smb::Connection, smb::Session, smb::Tree), String> {
    let port = cfg.port.unwrap_or(445);
    let transport =
        transport::HostTransport::open(tcp, &cfg.host, port).map_err(|error| error.to_string())?;
    let connection = smb::Connection::from_transport(
        Box::new(transport),
        &cfg.host,
        smb::Guid::generate(),
        smb::ConnectionConfig::default(),
    )
    .map_err(|error| error.to_string())?;
    let identity = sspi::AuthIdentity {
        username: sspi::Username::parse(&cfg.username).map_err(|error| error.to_string())?,
        password: sspi::Secret::from(cfg.password.clone()),
    };
    let session = connection
        .authenticate(identity)
        .map_err(|error| error.to_string())?;
    let unc = smb::UncPath::from_str(&format!("\\\\{}\\{}", cfg.host, cfg.share))
        .map_err(|error| error.to_string())?;
    let tree = session
        .tree_connect(&unc)
        .map_err(|error| error.to_string())?;
    Ok((connection, session, tree))
}

fn close(connection: smb::Connection, session: smb::Session, tree: smb::Tree) {
    let _ = tree.disconnect();
    let _ = session.logoff();
    let _ = connection.close();
}

fn open_stream(
    tcp: &sdk::TcpClient,
    cfg: &StoredConnection,
    path: &str,
) -> Result<OpenStream, RpcError> {
    let (connection, session, tree) = connect(tcp, cfg).map_err(RpcError::internal)?;
    let path = smb_path(cfg, path);
    let file = tree
        .create(
            &path,
            &smb::FileCreateArgs::make_open_existing(
                smb::FileAccessMask::new().with_generic_read(true),
            ),
        )
        .map_err(|error| RpcError::internal(error.to_string()))?
        .unwrap_file();
    let total = file
        .get_len()
        .map_err(|error| RpcError::internal(error.to_string()))?;
    Ok(OpenStream {
        connection,
        session,
        tree,
        file,
        total,
    })
}

fn safe_relative(path: &str) -> Result<(), RpcError> {
    if path.starts_with('/')
        || path.contains(['\\', '\0'])
        || path.split('/').any(|p| p == "." || p == "..")
    {
        return Err(RpcError::invalid_params("invalid remote path"));
    }
    Ok(())
}

fn smb_path(cfg: &StoredConnection, path: &str) -> String {
    [cfg.base.trim_matches('/'), path.trim_matches('/')]
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("\\")
}

fn display_name(path: &str) -> String {
    if path.is_empty() {
        "root".into()
    } else {
        path.trim_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or(path)
            .to_string()
    }
}

fn revision(file: &smb::File) -> Result<String, RpcError> {
    let basic = file
        .query_info::<smb::FileBasicInformation>()
        .map_err(fail)?;
    let size = file
        .query_info::<smb::FileStandardInformation>()
        .map_err(fail)?
        .end_of_file;
    Ok(format!(
        "{:?}:{:?}:{size}",
        basic.last_write_time, basic.change_time
    ))
}

fn connection(request: &FsRequest<'_>) -> Result<StoredConnection, RpcError> {
    serde_json::from_value(
        request
            .connection()
            .cloned()
            .ok_or_else(|| RpcError::invalid_params("missing connection"))?,
    )
    .map_err(|e| RpcError::invalid_params(e.to_string()))
}

fn directory_entries(
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

fn list(request: &FsRequest<'_>) -> Result<FsListing, RpcError> {
    let cfg = connection(request)?;
    let path = request.node_id().unwrap_or("").to_owned();
    safe_relative(&path)?;
    safe_relative(&cfg.base)?;
    let entries = directory_entries(&request.ctx().tcp(), &cfg, &path)?;
    let start = request
        .params()
        .get("cursor")
        .and_then(Value::as_str)
        .unwrap_or("0")
        .parse::<usize>()
        .map_err(|e| fail(e.to_string()))?;
    if start > entries.len() {
        return Err(fail("invalid pagination cursor"));
    }
    let limit = request
        .params()
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(200)
        .clamp(1, 500) as usize;
    let end = start.saturating_add(limit).min(entries.len());
    let next_cursor = (end < entries.len()).then(|| end.to_string());
    Ok(FsListing {
        entries: entries[start..end].to_vec(),
        next_cursor,
        warnings: Vec::new(),
        truncated: false,
    })
}

fn stat_path(
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

fn stat(request: &FsRequest<'_>) -> Result<FsEntry, RpcError> {
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

fn directory_revision(dir: &smb::Directory) -> Result<String, RpcError> {
    let basic = dir
        .query_info::<smb::FileBasicInformation>()
        .map_err(fail)?;
    let size = dir
        .query_info::<smb::FileStandardInformation>()
        .map_err(fail)?
        .end_of_file;
    Ok(format!(
        "{:?}:{:?}:{size}",
        basic.last_write_time, basic.change_time
    ))
}

fn resource_revision(resource: &smb::Resource) -> Result<String, RpcError> {
    if let Some(file) = resource.as_file() {
        revision(file)
    } else if let Some(dir) = resource.as_dir() {
        directory_revision(dir)
    } else {
        Err(fail("unsupported SMB resource"))
    }
}

fn mut_open(tree: &smb::Tree, target: &str) -> Result<smb::Resource, RpcError> {
    tree.create(
        target,
        &smb::FileCreateArgs::make_open_existing(smb::FileAccessMask::new().with_generic_all(true)),
    )
    .map_err(fail)
}

fn mkdir(request: &FsRequest<'_>) -> Result<FsEntry, RpcError> {
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

fn rename(request: &FsRequest<'_>) -> Result<FsEntry, RpcError> {
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
                return Err(fail("同步冲突：远端已修改"));
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

fn remove(request: &FsRequest<'_>) -> Result<(), RpcError> {
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
                return Err(fail("同步冲突：远端已修改"));
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
                return Err(fail("目录不为空，无法删除"));
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

fn valid_name(name: &str) -> Result<(), RpcError> {
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\', ':', '\0']) {
        return Err(RpcError::invalid_params("invalid name"));
    }
    Ok(())
}

fn join_path(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{}/{}", parent.trim_matches('/'), name)
    }
}

fn split_path(node: &str) -> (&str, &str) {
    node.rsplit_once('/').unwrap_or(("", node))
}

fn read(request: &FsRequest<'_>) -> Result<FsRead, RpcError> {
    let cfg = connection(request)?;
    let node = request
        .node_id()
        .ok_or_else(|| RpcError::invalid_params("missing node ID"))?;
    safe_relative(node)?;
    safe_relative(&cfg.base)?;
    let stream = open_stream(&request.ctx().tcp(), &cfg, node)?;
    let total = stream.total;
    Ok(FsRead::new(SmbReader { stream, offset: 0 }, Some(total)))
}

fn open(ctx: &sdk::RequestContext) -> Result<Value, RpcError> {
    let mount = ctx.params()["mountId"]
        .as_str()
        .ok_or_else(|| RpcError::invalid_params("missing mountId"))?;
    Ok(json!({"kind":"vfs","mountId":mount,"nodeRef":{"mountId":mount,"nodeId":""}}))
}

fn validate(ctx: &sdk::RequestContext) -> Result<Value, RpcError> {
    let cfg: StoredConnection = serde_json::from_value(
        ctx.params()
            .get("connection")
            .cloned()
            .ok_or_else(|| RpcError::invalid_params("missing connection"))?,
    )
    .map_err(|e| RpcError::invalid_params(e.to_string()))?;
    safe_relative(&cfg.base)?;
    directory_entries(&ctx.tcp(), &cfg, "")?;
    Ok(json!({"valid":true}))
}

struct Smb;

impl FSProvider for Smb {
    fn discover(&self, ctx: &sdk::RequestContext) -> Result<Vec<sdk::filesystem::FsOp>, RpcError> {
        discovery::discover(ctx)
    }

    fn operations(&self) -> &'static [sdk::filesystem::FsOp] {
        &[
            sdk::filesystem::FsOp::List,
            sdk::filesystem::FsOp::Stat,
            sdk::filesystem::FsOp::Read,
            sdk::filesystem::FsOp::Write,
            sdk::filesystem::FsOp::Mkdir,
            sdk::filesystem::FsOp::Rename,
            sdk::filesystem::FsOp::Remove,
        ]
    }

    fn list(&self, request: &FsRequest<'_>) -> Result<FsListing, RpcError> {
        list(request)
    }

    fn stat(&self, request: &FsRequest<'_>) -> Result<FsEntry, RpcError> {
        stat(request)
    }

    fn read(&self, request: &FsRequest<'_>) -> Result<FsRead, RpcError> {
        read(request)
    }

    fn write(&self, request: &FsRequest<'_>) -> Result<FsWrite, RpcError> {
        sync::write(request)
    }

    fn mkdir(&self, request: &FsRequest<'_>) -> Result<FsEntry, RpcError> {
        mkdir(request)
    }

    fn rename(&self, request: &FsRequest<'_>) -> Result<FsEntry, RpcError> {
        rename(request)
    }

    fn remove(&self, request: &FsRequest<'_>) -> Result<(), RpcError> {
        remove(request)
    }
}

impl sdk::ConnectionProvider for Smb {
    fn open(&self, ctx: &sdk::RequestContext) -> Result<serde_json::Value, sdk::RpcError> {
        open(ctx)
    }
    fn validate(&self, ctx: &sdk::RequestContext) -> Result<serde_json::Value, sdk::RpcError> {
        validate(ctx)
    }
}

fn main() {
    let providers = sdk::Providers::new().filesystem(Smb).connection(Smb);
    let result = providers.serve("org.fiex.smb");
    if let Err(error) = result {
        eprintln!("SMB plugin IPC failed: {error}");
        std::process::exit(1);
    }
}
