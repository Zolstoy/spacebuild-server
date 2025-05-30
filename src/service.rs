use crate::error::Error;
use crate::game::entity::Entity;
use crate::instance::Instance;
use crate::protocol::AuthInfo;
use crate::protocol::PlayerAction;
use crate::Id;
use futures::SinkExt;
use futures::StreamExt;
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::{Request, Response, StatusCode};
use hyper_tungstenite::tungstenite::Message;
use hyper_tungstenite::HyperWebsocket;
use log::error;
use log::info;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_stream::wrappers::ReceiverStream;
extern crate scopeguard;

use crate::Result;

pub async fn serve_http(
    mut request: Request<hyper::body::Incoming>,
    instance: Arc<Mutex<Instance>>,
    ws_hdl_sender: crossbeam::channel::Sender<tokio::task::JoinHandle<Result<()>>>,
) -> hyper::Result<Response<Full<Bytes>>> {
    let response_body = Full::<Bytes>::new("".into());
    let mut response = Response::<Full<Bytes>>::new(response_body);
    *response.status_mut() = StatusCode::BAD_REQUEST;

    if hyper_tungstenite::is_upgrade_request(&request) {
        info!("Upgrade request");
        let res = hyper_tungstenite::upgrade(&mut request, None);
        if res.is_err() {
            let err_str: String = res.err().unwrap().to_string();

            *response.body_mut() =
                Full::<Bytes>::new(format!("Can't upgrade to websocket: {}", err_str).into());

            info!("WS upgrade error");
            return Ok(response);
        }

        let (ws_resp, websocket) = res.unwrap();

        let hdl = tokio::spawn(async move {
            let instance_cln = Arc::clone(&instance);
            serve_websocket(websocket, instance_cln).await?;
            Ok(())
        });

        ws_hdl_sender.send(hdl).unwrap();

        return Ok(ws_resp);
    } else {
        *response.body_mut() = Full::<Bytes>::new(format!("Websocket only").into());
        info!("HTTP non WS request");
        return Ok(response);
    }
}

async fn serve_websocket(websocket: HyperWebsocket, instance: Arc<Mutex<Instance>>) -> Result<()> {
    let mut websocket = websocket.await.map_err(|_err| Error::WebSocketError)?;

    let mut id = Id::MAX;

    let mut authenticated = false;

    // let mut tick_delay = tokio::time::interval(std::time::Duration::from_millis(250));

    let recv = loop {
        tokio::select! {
            Some(message) = websocket.next() => {
                if message.is_err() {
                    info!("Websocket read error: {}", message.err().unwrap());
                    if id != u32::MAX {
                        instance.lock().await.leave(id).await?;
                    }
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

                            if let PlayerAction::Login(login) = maybe_login {
                                if authenticated {
                                    log::info!("{} already authenticated, closing him.", id);
                                    let _ = websocket.close(None).await;
                                    continue;
                                }
                                let mut guard = instance.lock().await;

                                info!("Login request for {}", login.nickname);
                                let maybe_uuid = guard.authenticate(&login.nickname).await;
                                if maybe_uuid.is_err() {
                                    login_info.message = format!("{}", maybe_uuid.err().unwrap());
                                    info!("Login error: {}", login_info.message);
                                    return Ok(())
                                }

                                let (player_id, infos_recv) = maybe_uuid.unwrap();

                                id = player_id;


                                info!("Login success for {}", id);
                                authenticated = true;

                                login_info.success = true;
                                login_info.message = id.to_string();


                                let maybe_login_info_str = serde_json::to_string(&login_info);
                                assert!(maybe_login_info_str.is_ok());
                                let result = websocket
                                    .send(Message::text(maybe_login_info_str.unwrap()))
                                    .await;
                                if result.is_err() {
                                    info!("Message send error: {}", result.err().unwrap());
                                }

                                break infos_recv;

                            } else {
                                log::info!("Client not authenticated, closing him");
                                let _ = websocket.close(None).await;
                                return Ok(());
                            }
                        }

                    }
                    Message::Binary(msg) => {
                        log::info!("{:?}", msg);
                    }
                    Message::Ping(msg) => {
                        log::info!("{:?}", msg);
                    }
                    Message::Pong(msg) => {
                        log::info!("{:?}", msg);
                    }
                    Message::Close(msg) => {
                        info!("WS close request received: {:?}", msg);
                        if id != Id::MAX {
                            instance.lock().await.leave(id).await?;
                        } else {
                            log::error!("Id is not assigned but closed received!");
                        }
                        return Ok(());
                    }
                    Message::Frame(msg) => {
                        log::info!("{:?}", msg);
                    }
                }
            }
        }
    };

    let mut stream = ReceiverStream::new(recv);

    loop {
        tokio::select! {
            game_info = stream.next() => {
                let str = serde_json::to_string(&game_info).unwrap();
                let result = websocket.send(Message::text(str)).await;
                if result.is_err() {
                    info!("Could not send data to client {}: {}", id, result.err().unwrap());
                    instance.lock().await.leave(id).await?;
                    let _ = websocket.close(None).await;
                    return Ok(());
                }
            },
            Some(message) = websocket.next() => {
                if message.is_err() {
                    info!("Websocket read error: {}", message.err().unwrap());
                    instance.lock().await.leave(id).await?;
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
                                if authenticated {
                                    log::info!("{} already authenticated, closing him.", id);
                                    let _ = websocket.close(None).await;
                                    return Ok(());
                                }
                            } else {
                                let mut instance = instance.lock().await;
                                let maybe_element = instance.borrow_galaxy_mut().borrow_body_mut(id);
                                if let Some(maybe_player) = maybe_element {
                                    if let Entity::Player(player) =
                                        &mut maybe_player.entity
                                    {
                                        player.actions.push(maybe_login);
                                    }
                                } else {
                                    error!("Can't find player {}", id);
                                }
                            }
                        }

                    }
                    Message::Binary(msg) => {
                        log::info!("{:?}", msg);
                    }
                    Message::Ping(msg) => {
                        log::info!("{:?}", msg);
                    }
                    Message::Pong(msg) => {
                        log::info!("{:?}", msg);
                    }
                    Message::Close(msg) => {
                        info!("WS close request received: {:?}", msg);
                        instance.lock().await.leave(id).await?;
                        return Ok(());
                    }
                    Message::Frame(msg) => {
                        log::info!("{:?}", msg);
                    }
                }
            }
        }
    }
}
