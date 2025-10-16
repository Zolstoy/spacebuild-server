use std::collections::HashMap;

use scilib::coordinate::{cartesian::Cartesian, spherical::Spherical};
use tokio::sync::mpsc::{error::TryRecvError, Receiver, Sender};

use crate::{
    body::Body,
    protocol::{self, Action},
    spacebuild_log,
};

pub struct Player {
    pub(crate) id: u32,
    pub(crate) nickname: String,
    pub(crate) coords: Cartesian,
    pub(crate) direction: Cartesian,
    pub(crate) current_system: u32,
    pub(crate) action_recv: Receiver<Action>,
    pub(crate) state_send: Sender<protocol::state::Game>,
    pub(crate) first_state_sent: bool,
    pub(crate) prev_lag_values: Vec<f64>,
    pub(crate) average_lag_value: f64,
}

impl PartialEq for Player {
    fn eq(&self, other: &Self) -> bool {
        self.nickname == other.nickname
    }
}

impl Player {
    pub(crate) fn new(
        nickname: String,
        state_send: Sender<protocol::state::Game>,
        action_recv: Receiver<Action>,
    ) -> Self {
        Self {
            average_lag_value: 0f64,
            id: 0,
            nickname,
            coords: Cartesian::default(),
            direction: Cartesian::default(),
            current_system: 0,
            action_recv,
            state_send,
            first_state_sent: false,
            prev_lag_values: Vec::new(),
        }
    }

    pub async fn update(&mut self, delta: f64, env: Vec<&Body>, history: &Vec<HashMap<u32, Body>>) {
        let mut direction = Cartesian::default();
        let mut throttle_up = false;

        loop {
            match self.action_recv.try_recv() {
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => unreachable!(),
                Ok(action) => match action {
                    Action::ShipState(ship_state) => {
                        if ship_state.throttle_up {
                            throttle_up = ship_state.throttle_up;
                            direction = Cartesian::from(
                                ship_state.direction[0],
                                ship_state.direction[1],
                                ship_state.direction[2],
                            );
                            direction /= direction.norm();
                        }
                    }
                    Action::Ping((entity_id, entity_rot_angle)) => {
                        for cache in history {
                            let ent = cache.get(&entity_id).unwrap();
                            let gravity_center = cache.get(&ent.gravity_center).unwrap();
                            let local_coordinates_car = ent.coords - gravity_center.coords;
                            let local_coordinates_sph = Spherical::from_coord(local_coordinates_car);
                            let delta_angle = entity_rot_angle - local_coordinates_sph.theta;
                            let lag_value = delta_angle / ent.rotating_speed;
                            self.prev_lag_values.push(lag_value);
                            self.average_lag_value =
                                self.prev_lag_values.iter().sum::<f64>() / self.prev_lag_values.len() as f64;
                        }
                    }
                    _ => todo!(),
                },
            }
        }

        // let mut coords = coordinates.clone();

        if direction.norm() > 0f64 {
            self.coords += direction / direction.norm() * 100f64 * delta;
        }

        if throttle_up || !self.first_state_sent {
            spacebuild_log!(trace, "player", "Sending ");
            let result = self
                .state_send
                .send(protocol::state::Game::Player(protocol::state::Player {
                    coords: [self.coords.x, self.coords.y, self.coords.z],
                }))
                .await;

            if result.is_err() {
                spacebuild_log!(warn, self.nickname, "Failed to send player info");
            }
        }

        if !self.first_state_sent {
            self.first_state_sent = true
        }
        let mut bodies: Vec<protocol::state::Body> = Vec::new();

        for celestial in env {
            bodies.push(celestial.clone().into());

            if bodies.len() == 50 {
                spacebuild_log!(
                    trace,
                    format!("{}:{}", self.id, self.nickname),
                    "Sending {} bodies state data",
                    bodies.len()
                );
                self.state_send
                    .send(protocol::state::Game::Env(bodies.clone()))
                    .await
                    .unwrap();
                bodies.clear();
            }
        }
        if !bodies.is_empty() {
            self.state_send
                .send(protocol::state::Game::Env(bodies.clone()))
                .await
                .unwrap();
        }

        // (coords, direction, speed)
    }
}
