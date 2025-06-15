use scilib::coordinate::cartesian::Cartesian;
use tokio::sync::mpsc::{Receiver, Sender};

use crate::{
    protocol::{GameState, PlayerAction, PlayerInfo},
    spacebuild_log,
};

pub struct Player {
    pub(crate) id: u32,
    pub(crate) nickname: String,
    pub(crate) coords: Cartesian,
    pub(crate) action_recv: Receiver<PlayerAction>,
    pub(crate) game_info_send: Sender<GameState>,
    pub(crate) first_state_sent: bool,
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

    pub async fn update(
        &mut self,
        coordinates: Cartesian,
        speed: f64,
        delta: f64,
        env: Vec<&Body>,
    ) -> (Cartesian, Cartesian, f64) {
        let mut direction = Cartesian::default();

        for action in &self.action_recv {
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

        if !self.action_recv.is_empty() || !self.first_state_sent {
            spacebuild_log!(trace, "player debug", "sending");
            let result = self
                .infos_send
                .send(GameState::Player(PlayerInfo {
                    coords: [coords.x, coords.y, coords.z],
                }))
                .await;

            if result.is_err() {
                spacebuild_log!(warn, self.nickname, "Failed to send player info");
            }
        }

        if !self.first_state_sent {
            self.first_state_sent = true
        }
        self.action_recv.clear();

        let mut bodies = Vec::new();

        for celestial in env {
            let element_type = match celestial.entity {
                super::Entity::Asteroid(_) => "Asteroid",
                super::Entity::Star(_) => "Star",
                super::Entity::Player(_) => "Player",
                super::Entity::Planet(_) => "Planet",
                super::Entity::Moon(_) => "Moon",
            };
            bodies.push(celestial.into());

            if bodies.len() == 50 {
                let result = self.infos_send.send(GameState::BodiesInSystem(bodies.clone())).await;
                if result.is_err() {
                    spacebuild_log!(warn, self.nickname, "Failed to send bodies in system info");
                }
                bodies.clear();
            }
        }

        if !bodies.is_empty() {
            let result = self.infos_send.send(GameState::BodiesInSystem(bodies.clone())).await;
            if result.is_err() {
                spacebuild_log!(warn, self.nickname, "Failed to send bodies in system info");
            }
        }

        (coords, direction, speed)
    }
}
