use crate::{
    game::{celestial_body::CelestialBody, repr::Vector3},
    protocol::{BodyInfo, GameInfo, PlayerAction, PlayerInfo},
    Id,
};

#[derive(Clone, Debug)]
pub struct Player {
    pub(crate) id: Id,
    pub(crate) nickname: String,
    pub(crate) _ownings: Vec<Id>,
    pub(crate) actions: Vec<PlayerAction>,
    pub(crate) infos_sender: tokio::sync::mpsc::Sender<GameInfo>,
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

    pub fn new(
        id: Id,
        nickname: String,
        infos_sender: tokio::sync::mpsc::Sender<GameInfo>,
    ) -> Player {
        Player {
            id,
            actions: Vec::default(),
            infos_sender,
            nickname,
            _ownings: Vec::default(),
        }
    }

    pub async fn update(
        &mut self,
        coordinates: Vector3,
        speed: f64,
        delta: f64,
        env: Vec<&CelestialBody>,
    ) -> (Vector3, Vector3, f64) {
        let mut direction = Vector3::default();

        for action in &self.actions {
            match action {
                PlayerAction::ShipState(ship_state) => {
                    if ship_state.throttle_up {
                        direction = Vector3::from(
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

        self.actions.clear();

        let mut coords = coordinates.clone();

        if direction.norm() > 0f64 {
            coords += direction / direction.norm() * speed * delta;
        }

        let _ = self
            .infos_sender
            .send(GameInfo::Player(PlayerInfo {
                coords: [coords.x, coords.y, coords.z],
            }))
            .await;

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
                let _ = self
                    .infos_sender
                    .send(GameInfo::BodiesInSystem(bodies.clone()))
                    .await;
                bodies.clear();
            }
        }

        if !bodies.is_empty() {
            let _ = self
                .infos_sender
                .send(GameInfo::BodiesInSystem(bodies.clone()))
                .await;
        }

        (coords, direction, speed)
    }
}
