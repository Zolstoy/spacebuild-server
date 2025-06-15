use crate::error::Error;
use crate::game::entity::Entity;
use crate::instance::Instance;
use crate::protocol::AuthInfo;
use crate::protocol::GameInfo;
use crate::protocol::PlayerAction;
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
    nickname: String,
    body_id: u32,
}

impl<S> Client<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    pub fn new(websocket: WebSocketStream<S>, instance: Arc<Mutex<Instance>>, address: SocketAddr) -> Client<S> {
        Client::<S> {
            websocket,
            instance,
            id: u32::MAX,
            address,
            nickname: String::default(),
            body_id: u32::MAX,
        }
    }

    async fn handle_message_for_auth(&mut self, message: Message) -> Result<Receiver<GameInfo>> {
        match message {
            Message::Text(msg) => {
                let maybe_action: serde_json::Result<PlayerAction> = serde_json::from_str(msg.as_str());

                let mut auth_info = AuthInfo {
                    success: false,
                    message: "".to_string(),
                };
                if maybe_action.is_err() {
                    auth_info.message = "Invalid JSON".to_string();
                    Err(Error::InvalidJson(maybe_action.err().unwrap()))
                } else {
                    let maybe_login = maybe_action.unwrap();

                    if let PlayerAction::Login(login) = maybe_login {
                        let mut guard = self.instance.lock().await;

                        spacebuild_log!(info, self.address, "Login request for {}", login.nickname);
                        let maybe_id_recv = guard.authenticate(&login.nickname).await;
                        if maybe_id_recv.is_err() {
                            auth_info.message = format!("{}", maybe_id_recv.err().unwrap());
                            spacebuild_log!(warn, self.address, "Login error: {}", auth_info.message);
                            return Err(Error::AuthenticationError(auth_info.message));
                        }

                        let (player_id, body_id, infos_recv) = maybe_id_recv.unwrap();

                        self.id = player_id;
                        self.body_id = body_id;
                        self.nickname = login.nickname.clone();

                        spacebuild_log!(debug, self.address, "Login success for {}", self.id);

                        auth_info.success = true;
                        auth_info.message = self.id.to_string();

                        let maybe_login_info_str = serde_json::to_string(&auth_info);
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
                    // let _ = self.mutex.lock().await;
                    let str = serde_json::to_string(&game_info).unwrap();
                    let result = self.websocket.send(Message::text(str)).await;
                    if result.is_err() {
                        spacebuild_log!(warn, self.address, "Could not send data to client {}: {}", self.id, result.err().unwrap());
                        self.instance.lock().await.leave(self.id, self.body_id).await?;
                        let _ = self.websocket.close(None).await;
                        return Ok(());
                    }
                },
                Some(message) = self.websocket.next() => {
                    // let _ = self.mutex.lock().await;
                    spacebuild_log!(trace, self.address, "Message received");
                    if message.is_err() {
                        spacebuild_log!(info, self.address, "Websocket read error: {}", message.err().unwrap());
                        self.instance.lock().await.leave(self.id, self.body_id).await?;
                        return Ok(());
                    }
                    match message.unwrap() {
                        Message::Text(msg) => {
                            let maybe_action: serde_json::Result<PlayerAction> =
                                serde_json::from_str(msg.as_str());

                            if maybe_action.is_err() {
                                spacebuild_log!(warn, self.address, "bad JSON received");
                                return Ok(());
                            }
                            let action = maybe_action.unwrap();

                            if let PlayerAction::ShipState(_state) = &action {
                                let mut instance = self.instance.lock().await;
                                let maybe_element = instance.borrow_galaxy_mut().borrow_body_mut(self.body_id);
                                if let Some(maybe_player) = maybe_element {
                                    if let Entity::Player(player) =
                                        &mut maybe_player.entity
                                    {
                                        player.actions.push(action);
                                    }
                                } else {
                                    panic!("Can't find player {}", self.id);
                                }
                            } else {
                                return Ok(())
                            }

                        }
                        Message::Close(_) => {
                            self.instance.lock().await.leave(self.id, self.body_id).await?;
                            return Ok(());
                        }
                        _ => {
                            spacebuild_log!(info, self.address, "Unexpected message type received: closing client");
                            self.instance.lock().await.leave(self.id, self.body_id).await?;
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
        let recv = self.handle_message_for_auth(message.unwrap()).await?;
        self.handle_message_for_gameplay(recv).await?;
        Ok(())
    }
}
