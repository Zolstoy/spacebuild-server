use crate::error::Error;
use crate::protocol::{Action, State};
use crate::Result;
use crate::{body::Body, player::Player, sqldb::SqlDb};
use is_printable::IsPrintable;
use scilib::coordinate::cartesian::Cartesian;
use sqlx::Row;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::sync::Mutex;

pub struct BodyCache {
    pub(crate) bodies: HashMap<u32, Body>,
    pub db: Arc<Mutex<SqlDb>>,
}

impl BodyCache {
    pub async fn get_body(&mut self, id: u32) -> &Body {
        self.bodies.get(&id).unwrap()
    }

    // async fn load_body(&self, id: u32) -> Body {
    //     self.db
    //         .lock()
    //         .await
    //         .select_from_where_equals("Body", "id", id.to_string().as_str())
    //         .await
    //         .first()
    //         .unwrap()
    //         .into()
    // }

    fn add_body(&mut self, id: u32, body: Body) -> &Body {
        self.bodies.insert(id, body);
        self.bodies.get(&id).unwrap()
    }

    pub(crate) async fn save(&self) -> () {
        let mut rows = vec![];

        for (_id, body) in &self.bodies {
            rows.push(vec![
                body.id.to_string(),
                body.coords.x.to_string(),
                body.coords.y.to_string(),
                body.coords.z.to_string(),
                body.rotating_speed.to_string(),
                body.gravity_center.to_string(),
                body.body_type.to_string(),
            ]);
        }
        if !rows.is_empty() {
            self.db.lock().await.insert_rows_into("Body", rows, vec![]).await;
        }
    }

    pub(crate) fn sync(&mut self, bodies: Vec<&Body>) -> () {
        for body in bodies {
            self.add_body(body.id, body.clone());
        }
    }

    pub(crate) async fn new_body(&mut self, body_type: u8) -> &mut Body {
        let mut new_body = Body {
            body_type,
            ..Default::default()
        };
        let id = {
            let mut db = self.db.lock().await;
            db.insert_row_into(
                "Body",
                vec![
                    0.to_string(),
                    new_body.body_type.to_string(),
                    new_body.coords.x.to_string(),
                    new_body.coords.y.to_string(),
                    new_body.coords.z.to_string(),
                    0f32.to_string(),
                    0.to_string(),
                ],
                vec![],
            )
            .await;
            new_body.id = db.last_insert_id().await;
            new_body.id
        };
        self.add_body(new_body.id, new_body);
        self.bodies.get_mut(&id).unwrap()
    }

    pub(crate) async fn new_bodies(&mut self, body_type: u8, cnt: i32) -> u32 {
        let mut new_bodies: Vec<Body> = Vec::new();
        let mut bodies_rows = Vec::new();
        for _ in 0..cnt {
            let new_body = Body {
                body_type,
                ..Default::default()
            };
            bodies_rows.push(vec![0.to_string(), body_type.to_string()]);
            new_bodies.push(new_body);
        }
        let last_id = {
            let db = self.db.lock().await;
            db.insert_rows_into("Body", bodies_rows, vec![]).await;
            db.last_insert_id().await
        };
        for i in cnt..0 {
            new_bodies.get_mut(i as usize).unwrap().id = last_id - (i as u32);
        }
        for i in 0..cnt {
            let body = new_bodies.get(i as usize).unwrap();
            self.bodies.insert(body.id, body.clone());
        }
        last_id
    }
}

pub struct PlayerCache {
    pub(crate) players: HashMap<u32, Player>,
    pub db: Arc<Mutex<SqlDb>>,
}

impl PlayerCache {
    pub async fn load_by_nickname(&mut self, nickname: String) -> Result<(u32, Sender<Action>, Receiver<State>)> {
        if nickname.is_empty() || !nickname.is_printable() {
            return Err(Error::InvalidNickname);
        }

        for (_, player) in &self.players {
            if player.nickname == nickname {
                return Err(Error::PlayerAlreadyAuthenticated);
            }
        }

        let query_result = self
            .db
            .lock()
            .await
            .select_from_where_equals("Player", "nickname", &nickname)
            .await;

        let (game_info_send, game_info_recv) = mpsc::channel(10000);
        let (action_send, action_recv) = mpsc::channel(10000);
        let player = if query_result.is_empty() {
            let mut player = Player {
                id: 0,
                nickname,
                coords: Cartesian::default(),
                action_recv,
                state_send: game_info_send,
                first_state_sent: false,
            };
            let id = self.save(&player).await;
            player.id = id;
            player
        } else {
            let first_row = query_result.first().unwrap();
            Player {
                id: first_row.get(0),
                nickname: first_row.get(0),
                coords: Cartesian::default(),
                action_recv,
                state_send: game_info_send,
                first_state_sent: false,
            }
        };

        Ok((player.id, action_send, game_info_recv))
    }

    async fn save(&mut self, _player: &Player) -> u32 {
        self.db.lock().await.insert_row_into("Player", vec![], vec![]).await
    }

    pub async fn sync(&mut self, id: u32) {
        let _player = self.players.get(&id);
        self.db
            .lock()
            .await
            .insert_row_into("Player", vec![id.to_string()], vec![])
            .await;
    }

    pub async fn sync_and_unload(&mut self, id: u32) {
        self.sync(id).await;
        self.players.remove(&id);
    }

    pub(crate) async fn new_player(&mut self, nickname: String) -> (&mut Player, Sender<Action>, Receiver<State>) {
        let (action_send, action_recv) = mpsc::channel(10000);
        let (state_send, state_recv) = mpsc::channel(10000);
        let mut new_player = Player {
            nickname: nickname.clone(),
            action_recv,
            state_send,
            coords: Cartesian::default(),
            first_state_sent: false,
            id: 0,
        };
        let id = {
            let mut db = self.db.lock().await;
            db.insert_row_into("Player", vec![0.to_string(), nickname], vec![])
                .await;
            new_player.id = db.last_insert_id().await;
            new_player.id
        };
        self.players.insert(id, new_player);
        let player = self.players.get_mut(&id).unwrap();
        (player, action_send, state_recv)
    }
}
