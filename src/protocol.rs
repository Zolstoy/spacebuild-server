use serde::{Deserialize, Serialize};
use tokio_tungstenite::tungstenite::Message;

use crate::error::Error;
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
pub enum Action {
    Login(Login),
    ShipState(ShipState),
}

pub mod state {
    use serde::{Deserialize, Serialize};

    use crate::body;

    #[derive(Serialize, Deserialize, Clone, Debug)]
    pub struct Player {
        pub coords: [f64; 3],
    }

    #[derive(Serialize, Deserialize, Clone, Debug, Default)]
    pub struct Body {
        pub id: u32,
        pub coords: [f64; 3],
        pub rotating_speed: f64,
        pub gravity_center: u32,
        pub body_type: String,
    }

    impl From<body::Body> for Body {
        fn from(value: body::Body) -> Self {
            Self {
                id: value.id,
                coords: [value.coords.x, value.coords.y, value.coords.z],
                gravity_center: value.gravity_center,
                rotating_speed: value.rotating_speed,
                body_type: value.body_type.to_string(),
            }
        }
    }
    #[derive(Serialize, Deserialize)]
    pub struct Auth {
        pub(crate) success: bool,
        pub(crate) message: String,
    }

    #[derive(Serialize, Deserialize, Clone, Debug)]
    pub enum Game {
        Player(Player),
        Env(Vec<Body>),
    }
}
