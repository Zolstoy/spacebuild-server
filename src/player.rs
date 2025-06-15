use scilib::coordinate::cartesian::Cartesian;
use tokio::sync::mpsc::{error::TryRecvError, Receiver, Sender};

use crate::{
    body::Body,
    protocol::{self, Action, PlayerState, State},
    spacebuild_log,
};

pub struct Player {
    pub(crate) id: u32,
    pub(crate) nickname: String,
    pub(crate) coords: Cartesian,
    pub(crate) action_recv: Receiver<Action>,
    pub(crate) state_send: Sender<State>,
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

        loop {
            match self.action_recv.try_recv() {
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => unreachable!(),
                Ok(action) => match action {
                    Action::ShipState(ship_state) => {
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
                },
            }
        }

        let mut coords = coordinates.clone();

        if direction.norm() > 0f64 {
            coords += direction / direction.norm() * speed * delta;
        }

        if !self.action_recv.is_empty() || !self.first_state_sent {
            spacebuild_log!(trace, "player debug", "sending");
            let result = self
                .state_send
                .send(State::Player(PlayerState {
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
        let mut bodies: Vec<protocol::Body> = Vec::new();

        for celestial in env {
            bodies.push(celestial.clone().into());

            if bodies.len() == 50 {
                self.state_send.send(State::Env(bodies.clone())).await.unwrap();
                bodies.clear();
            }
        }
        if !bodies.is_empty() {
            self.state_send.send(State::Env(bodies.clone())).await.unwrap();
        }

        (coords, direction, speed)
    }
}
