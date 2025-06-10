use crate::error::Error;
use crate::instance::Instance;
use crate::protocol::Action;
use futures::SinkExt;
use futures::StreamExt;
// use tokio_tungstenite::tungstenite::Message;
use hyper_tungstenite::tungstenite::Message;
use hyper_tungstenite::WebSocketStream;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::AsyncRead;
use tokio::io::AsyncWrite;
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::Sender;
use tokio::sync::Mutex;
use tokio_stream::wrappers::ReceiverStream;
// use tokio_tungstenite::WebSocketStream;
extern crate scopeguard;
use crate::spacebuild_log;
use crate::Result;

pub(crate) struct Service<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    id: u32,
    websocket: WebSocketStream<S>,
    address: SocketAddr,
    instance: Arc<Mutex<Instance>>,
}

impl<S> Service<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    pub fn new(websocket: WebSocketStream<S>, instance: Arc<Mutex<Instance>>, address: SocketAddr) -> Service<S> {
        Service::<S> {
            websocket,
            instance,
            id: u32::MAX,
            address,
        }
    }

    async fn handle_message_for_auth(
        &mut self,
        message: Message,
    ) -> Result<(Sender<Action>, Receiver<crate::protocol::state::Game>)> {
        match message {
            Message::Text(msg) => {
                let maybe_action: serde_json::Result<Action> = serde_json::from_str(msg.as_str());

                let mut auth_info = crate::protocol::state::Auth {
                    success: false,
                    message: "".to_string(),
                };
                if maybe_action.is_err() {
                    return Err(Error::InvalidJson(maybe_action.err().unwrap()));
                }

                let maybe_login = maybe_action.unwrap();

                if let Action::Login(login) = maybe_login {
                    let mut guard = self.instance.lock().await;

                    spacebuild_log!(info, self.address, "Login request for {}", login.nickname);
                    let maybe_data = guard.authenticate(login.nickname).await;
                    if maybe_data.is_err() {
                        auth_info.message = format!("{}", maybe_data.err().unwrap());
                        spacebuild_log!(warn, self.address, "Login error: {}", auth_info.message);
                        return Err(Error::AuthenticationError(auth_info.message));
                    }

                    let (id, action_send, state_recv) = maybe_data.unwrap();

                    self.id = id;

                    spacebuild_log!(debug, self.address, "Login success for {}", self.id);

                    auth_info.success = true;
                    auth_info.message = self.id.to_string();

                    let maybe_login_info_str = serde_json::to_string(&auth_info);
                    assert!(maybe_login_info_str.is_ok());
                    let result = self.websocket.send(Message::text(maybe_login_info_str.unwrap())).await;
                    if result.is_err() {
                        spacebuild_log!(warn, self.address, "Message send error: {}", result.err().unwrap());
                    }

                    Ok((action_send, state_recv))
                } else {
                    spacebuild_log!(warn, self.address, "Not an login action, closing client...");
                    let _ = self.websocket.close(None).await;
                    return Err(Error::NotALoginAction);
                }
            }
            _ => {
                spacebuild_log!(warn, self.address, "Not a text message, closing client...");
                let _ = self.websocket.close(None).await;
                Err(Error::NotTextMessage)
            }
        }
    }

    async fn handle_message_for_gameplay(
        &mut self,
        send: Sender<Action>,
        recv: Receiver<crate::protocol::state::Game>,
    ) -> Result<()> {
        let mut stream = ReceiverStream::new(recv);

        loop {
            tokio::select! {
                Some(game_info) = stream.next() => {
                    // let _ = self.mutex.lock().await;
                    let str = serde_json::to_string(&game_info).unwrap();
                    let result = self.websocket.send(Message::text(str)).await;
                    if result.is_err() {
                        spacebuild_log!(warn, self.address, "Could not send data to client {}: {}", self.id, result.err().unwrap());
                        self.instance.lock().await.leave(self.id).await;
                        let _ = self.websocket.close(None).await;
                        return Ok(());
                    }
                },
                Some(message) = self.websocket.next() => {
                    // let _ = self.mutex.lock().await;
                    spacebuild_log!(trace, self.address, "Message received");
                    if message.is_err() {
                        spacebuild_log!(info, self.address, "Websocket read error: {}", message.err().unwrap());
                        self.instance.lock().await.leave(self.id).await;
                        return Ok(());
                    }
                    match message.unwrap() {
                        Message::Text(msg) => {
                            let maybe_action: serde_json::Result<Action> =
                                serde_json::from_str(msg.as_str());

                            if maybe_action.is_err() {
                                spacebuild_log!(warn, self.address, "bad JSON received");
                                return Ok(());
                            }

                            send.send(maybe_action.unwrap()).await.unwrap();

                        }
                        Message::Close(_) => {
                            self.instance.lock().await.leave(self.id).await;
                            return Ok(());
                        }
                        _ => {
                            spacebuild_log!(info, self.address, "Unexpected message type received: closing client");
                            self.instance.lock().await.leave(self.id).await;
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
        let (send, recv) = self.handle_message_for_auth(message.unwrap()).await?;
        self.handle_message_for_gameplay(send, recv).await?;
        Ok(())
    }
}
