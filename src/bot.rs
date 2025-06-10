use crate::error::Error;
use crate::protocol::{IntoMessage, ShipState};
use crate::tls::{get_connector, ClientPki};
use crate::{
    protocol::{Action, Login},
    Result,
};
use futures::SinkExt;
use rustls_pki_types::ServerName;
use scilib::coordinate::cartesian::Cartesian;
use std::str::FromStr;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_stream::StreamExt;
use tokio_tungstenite::tungstenite::handshake::client::Request;
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message};
use tokio_tungstenite::WebSocketStream;

pub struct Bot<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    stream: WebSocketStream<S>,
}

impl<S: AsyncRead + AsyncWrite + Unpin> Bot<S> {
    async fn next_message(&mut self) -> Result<Message> {
        let message = self
            .stream
            .next()
            .await
            .ok_or_else(Error::WsNoMessage)?
            .map_err(|err| Error::WsCantRead(err))?;
        Ok(message)
    }
    pub async fn terminate(&mut self) -> Result<()> {
        self.stream
            .close(None)
            .await
            .map_err(|err| Error::GracefulCloseError(err))?;
        Ok(())
    }

    pub async fn login(&mut self, nickname: &str) -> Result<u32> {
        self.send_action(Action::Login(Login {
            nickname: nickname.to_string(),
        }))
        .await?;

        let response = self.next_message().await?;
        match response {
            Message::Text(response_str) => {
                let login_info: crate::protocol::state::Auth = serde_json::from_str(&response_str)
                    .map_err(|err| Error::DeserializeAuthenticationResponseError(err, response_str.to_string()))?;

                let uuid = u32::from_str(login_info.message.as_str())
                    .map_err(|_err| Error::BadUuidError(login_info.message))?;

                return Ok(uuid);
            }
            _ => return Err(Error::UnexpectedResponse(format!("{:?}", response))),
        }
    }

    async fn send_action<T: IntoMessage>(&mut self, action: T) -> Result<()> {
        self.stream
            .send(action.to_message()?)
            .await
            .map_err(|err| Error::WsCantSend(err))?;
        Ok(())
    }

    pub async fn move_in_space(&mut self, direction: Cartesian) -> Result<()> {
        self.send_action(Action::ShipState(ShipState {
            throttle_up: true,
            direction: [direction.x, direction.y, direction.z],
        }))
        .await?;
        Ok(())
    }

    pub async fn next_game_info(&mut self) -> Result<crate::protocol::state::Game> {
        let next = self.next_message().await?;

        match next {
            Message::Text(text) => {
                let game_info =
                    serde_json::from_str(&text).map_err(|err| Error::DeserializeError(text.to_string(), err))?;
                Ok(game_info)
            }
            _ => {
                unreachable!()
            }
        }
    }

    pub async fn until_player_info(&mut self) -> Result<crate::protocol::state::Player> {
        loop {
            let game_info = self.next_game_info().await?;

            if let crate::protocol::state::Game::Player(player) = game_info {
                return Ok(player);
            }
        }
    }
}

fn build_params(hostname: &str, port: u16, secure: bool) -> Result<(String, Request)> {
    let socket_addr = format!("{}:{}", hostname, port);
    let protocol = if secure { "wss" } else { "ws" };
    let url = format!("{}://{}", protocol, socket_addr);
    let request = url
        .clone()
        .into_client_request()
        .map_err(|_err| Error::UrlIntoRequest)?;
    Ok((socket_addr, request))
}

async fn connect_tcp(url: &str) -> Result<TcpStream> {
    let stream = TcpStream::connect(url)
        .await
        .map_err(|err| Error::TcpCouldNotConnect(err))?;
    Ok(stream)
}

async fn connect_websocket<S>(request: Request, stream: S) -> Result<Bot<S>>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    match tokio_tungstenite::client_async(request, stream).await {
        Ok((stream, _response)) => return Ok(Bot::<S> { stream }),
        Err(err) => {
            return Err(Error::WebSocketUpgrade(err));
        }
    }
}

pub async fn connect_secure(hostname: &str, port: u16, pki: ClientPki<'_>) -> Result<Bot<TlsStream<TcpStream>>> {
    let (socket_addr, request) = build_params(hostname, port, true)?;
    let stream = connect_tcp(socket_addr.as_str()).await?;

    let tls_connector = get_connector(pki)?;
    let stream = tls_connector
        .connect(
            ServerName::try_from("localhost").map_err(|err| Error::TlsHandshakeError(err))?,
            stream,
        )
        .await
        .map_err(|err| Error::CouldNotUpgradeToTls(err))?;

    let stream = connect_websocket(request, stream).await?;
    Ok(stream)
}

pub async fn connect_plain(hostname: &str, port: u16) -> Result<Bot<TcpStream>> {
    let (socket_addr, request) = build_params(hostname, port, true)?;
    let stream = connect_tcp(socket_addr.as_str()).await?;
    let stream = connect_websocket(request, stream).await?;
    Ok(stream)
}
