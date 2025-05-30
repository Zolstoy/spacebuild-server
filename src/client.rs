use std::str::FromStr;

use futures::{SinkExt, StreamExt};
use tokio::net::TcpStream;

use tokio_rustls::client::TlsStream;
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message};

use crate::error::Error;
use crate::game::repr::Vector3;
use crate::network::tcp::{connect, ClientStream};
use crate::network::tls::ClientPki;

use crate::protocol::{GameInfo, PlayerInfo, ShipState};
use crate::Id;
use crate::{
    protocol::{AuthInfo, Login, PlayerAction},
    Result,
};

enum WebSocketStream {
    Tls(tokio_tungstenite::WebSocketStream<TlsStream<TcpStream>>),
    Tcp(tokio_tungstenite::WebSocketStream<TcpStream>),
}

impl WebSocketStream {
    async fn send(&mut self, login_json: Message) -> Result<()> {
        match self {
            Self::Tcp(stream) => {
                stream
                    .send(login_json)
                    .await
                    .map_err(|err| Error::WsCantSend(err))?;
            }
            Self::Tls(stream) => {
                stream
                    .send(login_json)
                    .await
                    .map_err(|err| Error::WsCantSend(err))?;
            }
        }
        Ok(())
    }

    async fn next(&mut self) -> Result<Message> {
        let response = match self {
            Self::Tls(stream) => stream
                .next()
                .await
                .unwrap()
                .map_err(|_err| Error::NextTlsMessage)?,
            Self::Tcp(stream) => stream
                .next()
                .await
                .unwrap()
                .map_err(|_err| Error::NextTcpMessage)?,
        };
        Ok(response)
    }
}

pub struct Client {
    stream: WebSocketStream,
}

impl Client {
    pub async fn terminate(&mut self) -> Result<()> {
        match &mut self.stream {
            WebSocketStream::Tcp(stream) => {
                stream
                    .close(None)
                    .await
                    .map_err(|err| Error::GracefulCloseError(err))?;
            }
            WebSocketStream::Tls(stream) => {
                stream
                    .close(None)
                    .await
                    .map_err(|err| Error::GracefulCloseError(err))?;
            }
        }
        Ok(())
    }

    pub async fn connect(addr: &str, pki: Option<ClientPki<'_>>) -> Result<Client> {
        let mut host = if let None = pki {
            "ws://".to_string()
        } else {
            "wss://".to_string()
        };

        host += addr;

        let stream = connect(format!("{}", addr).as_str(), pki).await?;

        let request = host.into_client_request().unwrap();

        let (stream, _response) = match stream {
            ClientStream::Tcp(stream) => {
                match tokio_tungstenite::client_async(request, stream).await {
                    Ok((stream, response)) => (WebSocketStream::Tcp(stream), response),
                    Err(err) => {
                        return Err(Error::WebSocketUpgradeOnPlainError(err));
                    }
                }
            }
            ClientStream::Tls(stream) => {
                match tokio_tungstenite::client_async(request, stream).await {
                    Ok((stream, response)) => (WebSocketStream::Tls(stream), response),
                    Err(err) => {
                        return Err(Error::WebSocketUpgradeOnTlsError(err));
                    }
                }
            }
        };

        Ok(Client { stream })
    }

    pub async fn login(&mut self, nickname: &str) -> Result<Id> {
        let login = PlayerAction::Login(Login {
            nickname: nickname.to_string(),
        });
        let login_json =
            serde_json::to_string(&login).map_err(|err| Error::FailedToSerializeLogin(err))?;

        self.stream.send(Message::Text(login_json.into())).await?;

        let response = self.stream.next().await?;

        match response {
            Message::Text(response_str) => {
                let login_info: AuthInfo = serde_json::from_str(&response_str).map_err(|err| {
                    Error::DeserializeAuthenticationResponseError(err, response_str.to_string())
                })?;

                let uuid = Id::from_str(login_info.message.as_str())
                    .map_err(|_err| Error::BadUuidError(login_info.message))?;

                return Ok(uuid);
            }
            _ => return Err(Error::UnexpectedResponse(format!("{:?}", response))),
        }
    }

    pub async fn move_in_space(&mut self, direction: Vector3) -> Result<()> {
        self.stream
            .send(Message::Text(
                serde_json::to_string(&PlayerAction::ShipState(ShipState {
                    throttle_up: true,
                    direction: [direction.x, direction.y, direction.z],
                }))
                .unwrap()
                .into(),
            ))
            .await?;
        Ok(())
    }

    pub async fn next_game_info(&mut self) -> Result<GameInfo> {
        let next = self.stream.next().await?;

        match next {
            Message::Text(text) => {
                let game_info = serde_json::from_str(&text)
                    .map_err(|err| Error::DeserializeError(text.to_string(), err))?;
                Ok(game_info)
            }
            _ => {
                unreachable!()
            }
        }
    }

    pub async fn until_player_info(&mut self) -> Result<PlayerInfo> {
        loop {
            let game_info = self.next_game_info().await?;

            if let GameInfo::Player(player) = game_info {
                return Ok(player);
            }
        }
    }
}
