use fiex_plugin_sdk::{tcp::TcpStream, worker::HostProxy};
use smb::transport::{SmbTransport, SmbTransportRead, SmbTransportWrite, TransportError};
use std::{
    io::{Read, Write},
    net::SocketAddr,
    time::Duration,
};
pub struct HostTransport {
    stream: TcpStream,
}
impl HostTransport {
    pub fn open(host: HostProxy, name: &str, port: u16) -> Result<Self, TransportError> {
        Ok(Self {
            stream: TcpStream::connect(host, name, port)?,
        })
    }
}
impl SmbTransportRead for HostTransport {
    fn receive_exact(&mut self, output: &mut [u8]) -> Result<(), TransportError> {
        self.stream.read_exact(output).map_err(Into::into)
    }
    fn set_read_timeout(&self, _timeout: Duration) -> Result<(), TransportError> {
        Ok(())
    }
}
impl SmbTransportWrite for HostTransport {
    fn send_raw(&mut self, data: &[u8]) -> Result<(), TransportError> {
        self.stream.write_all(data).map_err(Into::into)
    }
}
impl SmbTransport for HostTransport {
    fn connect(&mut self, _name: &str, _address: SocketAddr) -> Result<(), TransportError> {
        Ok(())
    }
    fn default_port(&self) -> u16 {
        445
    }
    fn remote_address(&self) -> Result<SocketAddr, TransportError> {
        Ok(self.stream.peer_addr())
    }
    fn split(
        self: Box<Self>,
    ) -> Result<(Box<dyn SmbTransportRead>, Box<dyn SmbTransportWrite>), TransportError> {
        Ok((
            Box::new(Self {
                stream: self.stream.clone(),
            }),
            self,
        ))
    }
}
