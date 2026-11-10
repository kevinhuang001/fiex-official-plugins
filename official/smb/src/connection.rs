use super::*;
use serde::Deserialize;

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StoredConnection {
    pub(super) host: String,
    pub(super) share: String,
    #[serde(default)]
    pub(super) base: String,
    #[serde(default)]
    pub(super) username: String,
    #[serde(default)]
    pub(super) password: String,
    pub(super) port: Option<u16>,
}

/// A real remote file kept open for the lifetime of one `vfs.openRead` stream.
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

pub(super) fn fail(e: impl std::fmt::Display) -> RpcError {
    RpcError::internal(e.to_string())
}

pub(super) fn connect(
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

pub(super) fn close(connection: smb::Connection, session: smb::Session, tree: smb::Tree) {
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

pub(super) fn read_stream(
    tcp: &sdk::TcpClient,
    cfg: &StoredConnection,
    path: &str,
) -> Result<FsRead, RpcError> {
    let stream = open_stream(tcp, cfg, path)?;
    let total = stream.total;
    Ok(FsRead::new(SmbReader { stream, offset: 0 }, Some(total)))
}

pub(super) fn safe_relative(path: &str) -> Result<(), RpcError> {
    if path.starts_with('/')
        || path.contains(['\\', '\0'])
        || path.split('/').any(|p| p == "." || p == "..")
    {
        return Err(RpcError::invalid_params("invalid remote path"));
    }
    Ok(())
}

pub(super) fn smb_path(cfg: &StoredConnection, path: &str) -> String {
    [cfg.base.trim_matches('/'), path.trim_matches('/')]
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("\\")
}

pub(super) fn display_name(path: &str) -> String {
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

pub(super) fn revision(file: &smb::File) -> Result<String, RpcError> {
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

pub(super) fn connection(request: &FsRequest<'_>) -> Result<StoredConnection, RpcError> {
    let connection = request
        .connection_value()?
        .ok_or_else(|| RpcError::invalid_params("missing connection"))?;
    serde_json::from_value(connection).map_err(|e| RpcError::invalid_params(e.to_string()))
}

pub(super) fn directory_revision(dir: &smb::Directory) -> Result<String, RpcError> {
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

pub(super) fn resource_revision(resource: &smb::Resource) -> Result<String, RpcError> {
    if let Some(file) = resource.as_file() {
        revision(file)
    } else if let Some(dir) = resource.as_dir() {
        directory_revision(dir)
    } else {
        Err(fail("unsupported SMB resource"))
    }
}

pub(super) fn mut_open(tree: &smb::Tree, target: &str) -> Result<smb::Resource, RpcError> {
    tree.create(
        target,
        &smb::FileCreateArgs::make_open_existing(smb::FileAccessMask::new().with_generic_all(true)),
    )
    .map_err(fail)
}

pub(super) fn valid_name(name: &str) -> Result<(), RpcError> {
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\', ':', '\0']) {
        return Err(RpcError::invalid_params("invalid name"));
    }
    Ok(())
}

pub(super) fn join_path(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{}/{}", parent.trim_matches('/'), name)
    }
}

pub(super) fn split_path(node: &str) -> (&str, &str) {
    node.rsplit_once('/').unwrap_or(("", node))
}
