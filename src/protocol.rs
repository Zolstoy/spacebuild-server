use serde::{Deserialize, Serialize};
use tokio_tungstenite::tungstenite::Message;

use crate::error::Error;
use crate::Id;
use crate::Result;

pub trait IntoMessage {
    fn to_message(&self) -> Result<Message>;
}

impl<T> IntoMessage for T
where
    T: Serialize,
{
    fn to_message(&self) -> Result<Message> {
        let json = serde_json::to_string(self).map_err(|err| Error::FailedToSerializeLogin(err))?;
        Ok(Message::text(json))
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Login {
    pub nickname: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ShipState {
    pub throttle_up: bool,
    pub direction: [f64; 3],
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum PlayerAction {
    Login(Login),
    ShipState(ShipState),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PlayerInfo {
    pub coords: [f64; 3],
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct BodyInfo {
    pub coords: [f64; 3],
    pub rotating_speed: f64,
    pub gravity_center: Id,
    pub id: Id,
    pub element_type: String,
}

#[derive(Serialize, Deserialize)]
pub struct AuthInfo {
    pub(crate) success: bool,
    pub(crate) message: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum GameInfo {
    Player(PlayerInfo),
    BodiesInSystem(Vec<BodyInfo>),
    // PlayersInSystem(Vec<PlayerInfo>),
}
