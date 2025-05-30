use rustls_pki_types::ServerName;

use tokio::net::TcpStream;

use crate::error::Error;
use crate::Result;

use super::tls::{get_connector, ClientPki};

pub enum ServerStream {
    Tls(tokio_rustls::server::TlsStream<TcpStream>),
    Tcp(TcpStream),
}

pub enum ClientStream {
    Tls(tokio_rustls::client::TlsStream<TcpStream>),
    Tcp(TcpStream),
}

pub async fn connect(addr: &str, pki: Option<ClientPki<'_>>) -> Result<ClientStream> {
    let stream = TcpStream::connect(addr)
        .await
        .map_err(|err| Error::TcpCouldNotConnect(err))?;

    if let None = pki {
        return Ok(ClientStream::Tcp(stream));
    }

    let tls_connector = get_connector(pki.unwrap())?;

    let stream = tls_connector
        .connect(
            ServerName::try_from("localhost").map_err(|err| Error::TlsHandshakeError(err))?,
            stream,
        )
        .await
        .map_err(|err| Error::CouldNotUpgradeToTls(err))?;
    return Ok(ClientStream::Tls(stream));
}
