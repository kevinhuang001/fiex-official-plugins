//! TLS over a Core-brokered TCP stream. The plugin never opens a socket.
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use std::io::{self, Read, Write};
use std::sync::Arc;

fn config() -> io::Result<Arc<ClientConfig>> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    Ok(Arc::new(
        ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth(),
    ))
}

/// Wrap any byte stream (Core's SDK TCP stream or a direct test socket) in
/// rustls. Core still owns authorization of the underlying TCP connection.
pub fn wrap<S: Read + Write>(
    stream: S,
    host: &str,
) -> io::Result<StreamOwned<ClientConnection, S>> {
    let name = ServerName::try_from(host.to_string())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.to_string()))?;
    let connection = ClientConnection::new(config()?, name)
        .map_err(|error| io::Error::other(error.to_string()))?;
    Ok(StreamOwned::new(connection, stream))
}
