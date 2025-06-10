use crate::error::Error;
use crate::game::entity::Entity;
use crate::instance::Instance;
use crate::protocol::AuthInfo;
use crate::protocol::GameInfo;
use crate::protocol::PlayerAction;
use crate::Id;
use futures::SinkExt;
use futures::StreamExt;
use hyper_tungstenite::tungstenite::Message;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::AsyncRead;
use tokio::io::AsyncWrite;
use tokio::sync::mpsc::Receiver;
use tokio::sync::Mutex;
use tokio_stream::wrappers::ReceiverStream;
use tokio_tungstenite::WebSocketStream;
extern crate scopeguard;
use crate::spacebuild_log;
use crate::Result;

pub(crate) struct Client<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    websocket: WebSocketStream<S>,
    instance: Arc<Mutex<Instance>>,
    id: u32,
    address: SocketAddr,
}

impl<S> Client<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    pub fn new(websocket: WebSocketStream<S>, instance: Arc<Mutex<Instance>>, address: SocketAddr) -> Client<S> {
        Client::<S> {
            websocket,
            instance,
            id: Id::MAX,
            address,
        }
    }

    async fn handle_message_for_auth(&mut self, message: Message) -> Result<Receiver<GameInfo>> {
        match message {
            Message::Text(msg) => {
                let maybe_action: serde_json::Result<PlayerAction> = serde_json::from_str(msg.as_str());

                let mut login_info = AuthInfo {
                    success: false,
                    message: "".to_string(),
                };
                if maybe_action.is_err() {
                    login_info.message = "Invalid JSON".to_string();
                    Err(Error::InvalidJson(maybe_action.err().unwrap()))
                } else {
                    let maybe_login = maybe_action.unwrap();

                    if let PlayerAction::Login(login) = maybe_login {
                        let mut guard = self.instance.lock().await;

                        spacebuild_log!(info, self.address, "Login request for {}", login.nickname);
                        let maybe_uuid = guard.authenticate(&login.nickname).await;
                        if maybe_uuid.is_err() {
                            login_info.message = format!("{}", maybe_uuid.err().unwrap());
                            spacebuild_log!(warn, self.address, "Login error: {}", login_info.message);
                            return Err(Error::AuthenticationError(login_info.message));
                        }

                        let (player_id, infos_recv) = maybe_uuid.unwrap();

                        self.id = player_id;

                        spacebuild_log!(debug, self.address, "Login success for {}", self.id);

                        login_info.success = true;
                        login_info.message = self.id.to_string();

                        let maybe_login_info_str = serde_json::to_string(&login_info);
                        assert!(maybe_login_info_str.is_ok());
                        let result = self.websocket.send(Message::text(maybe_login_info_str.unwrap())).await;
                        if result.is_err() {
                            spacebuild_log!(warn, self.address, "Message send error: {}", result.err().unwrap());
                        }

                        Ok(infos_recv)
                    } else {
                        spacebuild_log!(warn, self.address, "Client not authenticated, closing him");
                        let _ = self.websocket.close(None).await;
                        return Err(Error::PlayerNotAuthenticated);
                    }
                }
            }
            _ => Err(Error::NotTextMessage),
        }
    }

    async fn handle_message_for_gameplay(&mut self, recv: Receiver<GameInfo>) -> Result<()> {
        let mut stream = ReceiverStream::new(recv);

        loop {
            tokio::select! {
                Some(game_info) = stream.next() => {
                    let str = serde_json::to_string(&game_info).unwrap();
                    let result = self.websocket.send(Message::text(str)).await;
                    if result.is_err() {
                        spacebuild_log!(warn, self.address, "Could not send data to client {}: {}", self.id, result.err().unwrap());
                        self.instance.lock().await.leave(self.id).await?;
                        let _ = self.websocket.close(None).await;
                        return Ok(());
                    }
                },
                Some(message) = self.websocket.next() => {
                    spacebuild_log!(trace, self.address, "Message received");
                    if message.is_err() {
                        spacebuild_log!(info, self.address, "Websocket read error: {}", message.err().unwrap());
                        self.instance.lock().await.leave(self.id).await?;
                        return Ok(());
                    }
                    match message.unwrap() {
                        Message::Text(msg) => {
                            let maybe_action: serde_json::Result<PlayerAction> =
                                serde_json::from_str(msg.as_str());

                            let mut login_info = AuthInfo {
                                success: false,
                                message: "".to_string(),
                            };
                            if maybe_action.is_err() {
                                login_info.message = "Invalid JSON".to_string();
                            } else {
                                let maybe_login = maybe_action.unwrap();

                                if let PlayerAction::Login(_login) = maybe_login {
                                        spacebuild_log!(info, self.address, "{} already authenticated, closing him.", self.id);
                                        let _ = self.websocket.close(None).await;
                                        return Ok(());
                                } else {
                                    let mut instance = self.instance.lock().await;
                                    let maybe_element = instance.borrow_galaxy_mut().borrow_body_mut(self.id);
                                    if let Some(maybe_player) = maybe_element {
                                        if let Entity::Player(player) =
                                            &mut maybe_player.entity
                                        {
                                            player.actions.push(maybe_login);
                                        }
                                    } else {
                                        spacebuild_log!(error, self.address, "Can't find player {}", self.id);
                                    }
                                }
                            }

                        }
                        _ => {
                            spacebuild_log!(info, self.address, "Unexpected message type received: closing client");
                            self.instance.lock().await.leave(self.id).await?;
                            return Ok(());
                        }
                    }
                }
            }
        }
    }

    pub async fn serve(&mut self) -> Result<()> {
        spacebuild_log!(trace, self.address, "About to serve gameplay");
        let message = self.websocket.next().await;
        if message.is_none() {
            return Ok(());
        }
        let message = message.unwrap();
        // if message.is_err() {
        //     spacebuild_log!(info, self.address, "Websocket read error: {}", message.err().unwrap());
        //     if self.id != u32::MAX {
        //         self.instance.lock().await.leave(self.id).await?;
        //     }
        //     return Ok(());
        // }
        let recv = self.handle_message_for_auth(message.unwrap()).await?;
        self.handle_message_for_gameplay(recv).await?;
        Ok(())
    }
}
