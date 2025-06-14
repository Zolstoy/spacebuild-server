use scilib::coordinate::cartesian::Cartesian;
use tokio::sync::mpsc::Sender;

use crate::{
    game::celestial_body::CelestialBody,
    protocol::{BodyInfo, GameInfo, PlayerAction, PlayerInfo},
    spacebuild_log,
};

#[derive(Clone, Debug)]
pub struct Player {
    pub(crate) id: u32,
    pub(crate) nickname: String,
    pub(crate) _ownings: Vec<u32>,
    pub(crate) actions: Vec<PlayerAction>,
    pub(crate) infos_sender: Sender<GameInfo>,
    pub(crate) initialized: bool,
}

impl PartialEq for Player {
    fn eq(&self, other: &Self) -> bool {
        self.nickname == other.nickname
    }
}

impl Player {
    pub fn borrow_nickname(&self) -> &String {
        &self.nickname
    }

    pub fn new(id: u32, nickname: String, infos_sender: Sender<GameInfo>) -> Player {
        Player {
            id,
            actions: Vec::default(),
            infos_sender,
            nickname,
            _ownings: Vec::default(),
            initialized: false,
        }
    }

    pub async fn update(
        &mut self,
        coordinates: Cartesian,
        speed: f64,
        delta: f64,
        env: Vec<&CelestialBody>,
    ) -> (Cartesian, Cartesian, f64) {
        let mut direction = Cartesian::default();

        for action in &self.actions {
            match action {
                PlayerAction::ShipState(ship_state) => {
                    if ship_state.throttle_up {
                        direction = Cartesian::from(
                            ship_state.direction[0],
                            ship_state.direction[1],
                            ship_state.direction[2],
                        );
                        direction /= direction.norm();
                    }
                }
                _ => todo!(),
            }
        }

        let mut coords = coordinates.clone();

        if direction.norm() > 0f64 {
            coords += direction / direction.norm() * speed * delta;
        }

        if !self.actions.is_empty() || !self.initialized {
            spacebuild_log!(trace, "player debug", "sending");
            let result = self
                .infos_sender
                .send(GameInfo::Player(PlayerInfo {
                    coords: [coords.x, coords.y, coords.z],
                }))
                .await;

            if result.is_err() {
                spacebuild_log!(warn, self.nickname, "Failed to send player info");
            }
        }

        if !self.initialized {
            self.initialized = true
        }
        self.actions.clear();

        let mut bodies = Vec::new();

        for celestial in env {
            let element_type = match celestial.entity {
                super::Entity::Asteroid(_) => "Asteroid",
                super::Entity::Star(_) => "Star",
                super::Entity::Player(_) => "Player",
                super::Entity::Planet(_) => "Planet",
                super::Entity::Moon(_) => "Moon",
            };
            bodies.push(BodyInfo {
                coords: [celestial.coords.x, celestial.coords.y, celestial.coords.z],
                id: celestial.id,
                element_type: element_type.to_string(),
                gravity_center: celestial.gravity_center,
                rotating_speed: celestial.rotating_speed,
            });

            if bodies.len() == 50 {
                let result = self.infos_sender.send(GameInfo::BodiesInSystem(bodies.clone())).await;
                if result.is_err() {
                    spacebuild_log!(warn, self.nickname, "Failed to send bodies in system info");
                }
                bodies.clear();
            }
        }

        if !bodies.is_empty() {
            let result = self.infos_sender.send(GameInfo::BodiesInSystem(bodies.clone())).await;
            if result.is_err() {
                spacebuild_log!(warn, self.nickname, "Failed to send bodies in system info");
            }
        }

        (coords, direction, speed)
    }
}
