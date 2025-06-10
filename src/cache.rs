use crate::error::Error;
use crate::protocol::Action;
use crate::{body::Body, player::Player, sqldb::SqlDb};
use crate::{spacebuild_log, Result};
use is_printable::IsPrintable;
use sqlx::Row;
use std::vec;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::sync::Mutex;

pub struct BodyCache {
    pub(crate) cache: HashMap<u32, Body>,
    db: Arc<Mutex<SqlDb>>,
}

impl BodyCache {
    pub fn new(db: Arc<Mutex<SqlDb>>) -> Self {
        Self {
            cache: HashMap::new(),
            db: db.clone(),
        }
    }

    pub(crate) async fn init_db(&mut self) {
        self.db
            .lock()
            .await
            .create_table(
                "Body",
                vec![
                    "id INTEGER PRIMARY KEY AUTOINCREMENT",
                    "type INTEGER",
                    "coord_x REAL",
                    "coord_y REAL",
                    "coord_z REAL",
                    "rotating_speed REAL",
                    "gravity_center INTEGER",
                    "FOREIGN KEY (gravity_center) REFERENCES Body (id)",
                ],
                vec!["id", "gravity_center"],
            )
            .await
            .unwrap();
    }

    pub fn get_body(&mut self, id: u32) -> &Body {
        self.cache.get(&id).unwrap()
    }

    pub(crate) async fn load_body(&mut self, id: u32) -> &Body {
        if self.cache.contains_key(&id) {
            return self.cache.get(&id).unwrap();
        }
        let body: Body = self
            .db
            .lock()
            .await
            .select_from_where_equals("Body", "id", id.to_string().as_str())
            .await
            .first()
            .unwrap()
            .into();

        self.cache.insert(id, body);
        self.cache.get(&id).unwrap()
    }

    pub(crate) fn add_body(&mut self, id: u32, body: Body) -> &Body {
        self.cache.insert(id, body);
        self.cache.get(&id).unwrap()
    }

    pub(crate) async fn save_all(&self) -> () {
        let mut rows = vec![];

        for (_id, body) in &self.cache {
            rows.push(vec![
                body.id.to_string(),
                body.body_type.to_string(),
                body.coords.x.to_string(),
                body.coords.y.to_string(),
                body.coords.z.to_string(),
                body.rotating_speed.to_string(),
                body.gravity_center.to_string(),
            ]);
        }
        if !rows.is_empty() {
            self.db
                .lock()
                .await
                .insert_rows_into(
                    "Body",
                    Some(vec![
                        "id".to_string(),
                        "type".to_string(),
                        "coord_x".to_string(),
                        "coord_y".to_string(),
                        "coord_z".to_string(),
                        "rotating_speed".to_string(),
                        "gravity_center".to_string(),
                    ]),
                    rows,
                    vec![
                        ("type", "type"),
                        ("coord_x", "coord_x"),
                        ("coord_y", "coord_y"),
                        ("coord_z", "coord_z"),
                        ("rotating_speed", "rotating_speed"),
                        ("gravity_center", "gravity_center"),
                    ],
                )
                .await;
        }
    }

    pub async fn load_gravitings(&mut self, id: u32) -> Vec<Body> {
        let mut ids = vec![id];
        let mut prev_ids = vec![];
        let mut bodies: Vec<Body> = Vec::new();
        while !ids.is_empty() {
            let id = ids.pop().unwrap();
            prev_ids.push(id);
            let body_rows = self
                .db
                .lock()
                .await
                .select_from_where_equals("Body", "gravity_center", id.to_string().as_str())
                .await;
            for row in body_rows {
                let body: Body = (&row).into();
                if !ids.contains(&body.id) && !prev_ids.contains(&body.id) {
                    ids.push(body.id);
                }
                if !self.cache.contains_key(&body.id) {
                    bodies.push(body);
                }
            }
        }
        bodies
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
            new_body.id = db
                .insert_row_into(
                    "Body",
                    Some(vec![
                        "type".to_string(),
                        "coord_x".to_string(),
                        "coord_y".to_string(),
                        "coord_z".to_string(),
                    ]),
                    vec![
                        new_body.body_type.to_string(),
                        new_body.coords.x.to_string(),
                        new_body.coords.y.to_string(),
                        new_body.coords.z.to_string(),
                    ],
                    vec![],
                )
                .await;
            new_body.id
        };
        self.add_body(new_body.id, new_body);
        self.cache.get_mut(&id).unwrap()
    }

    pub(crate) async fn new_bodies(&mut self, body_type: u8, cnt: i32) -> u32 {
        let mut new_bodies: Vec<Body> = Vec::new();
        let mut bodies_rows = Vec::new();
        for _ in 0..cnt {
            let new_body = Body {
                body_type,
                ..Default::default()
            };
            bodies_rows.push(vec![
                body_type.to_string(),
                new_body.coords.x.to_string(),
                new_body.coords.y.to_string(),
                new_body.coords.z.to_string(),
            ]);
            new_bodies.push(new_body);
        }
        let last_id = {
            let db = self.db.lock().await;
            db.insert_rows_into(
                "Body",
                Some(vec![
                    "type".to_string(),
                    "coord_x".to_string(),
                    "coord_y".to_string(),
                    "coord_z".to_string(),
                ]),
                bodies_rows,
                vec![],
            )
            .await
        };
        for i in 0..cnt {
            let body = new_bodies.get_mut(i as usize).unwrap();
            body.id = last_id - i as u32;
            self.cache.insert(last_id - i as u32, body.clone());
        }
        last_id
    }
}

pub struct PlayerCache {
    pub(crate) cache: HashMap<u32, Player>,
    pub db: Arc<Mutex<SqlDb>>,
}

impl PlayerCache {
    pub(crate) async fn init_db(&mut self) {
        self.db
            .lock()
            .await
            .create_table(
                "Player",
                vec![
                    "id INTEGER PRIMARY KEY",
                    "nickname TEXT",
                    "coord_x REAL",
                    "coord_y REAL",
                    "coord_z REAL",
                    "direction_x REAL",
                    "direction_y REAL",
                    "direction_z REAL",
                    "current_system INTEGER",
                ],
                vec!["id", "nickname"],
            )
            .await
            .unwrap();
    }

    pub fn get_player(&mut self, id: u32) -> &Player {
        self.cache.get(&id).unwrap()
    }

    pub async fn can_login(&mut self, nickname: String) -> Result<()> {
        if nickname.is_empty() || !nickname.is_printable() {
            return Err(Error::InvalidNickname);
        }

        for (_, player) in &self.cache {
            if player.nickname == nickname {
                return Err(Error::PlayerAlreadyAuthenticated);
            }
        }

        let query_result = self
            .db
            .lock()
            .await
            .select_from_where_like("Player", "nickname", &nickname)
            .await;

        if query_result.is_empty() {
            return Err(Error::PlayerIsNew);
        };
        Ok(())
    }

    // pub async fn sync(&mut self, id: u32) -> u32 {
    //     let player = self.cache.get(&id).unwrap();
    //     self.db
    //         .lock()
    //         .await
    //         .insert_row_into(
    //             "Player",
    //             Some(vec![
    //                 "id".to_string(),
    //                 "nickname".to_string(),
    //                 "coord_x".to_string(),
    //                 "coord_y".to_string(),
    //                 "coord_z".to_string(),
    //             ]),
    //             vec![
    //                 id.to_string(),
    //                 format!("\"{}\"", player.nickname),
    //                 player.coords.x.to_string(),
    //                 player.coords.y.to_string(),
    //                 player.coords.z.to_string(),
    //             ],
    //             vec![("coord_x", "coord_x"), ("coord_y", "coord_y"), ("coord_z", "coord_z")],
    //         )
    //         .await
    // }

    pub async fn sync_and_unload(&mut self, id: u32) {
        self.save(id).await;
        self.cache.remove(&id);
    }

    pub(crate) async fn load(
        &mut self,
        nickname: String,
    ) -> (u32, Sender<Action>, Receiver<crate::protocol::state::Game>) {
        let result = self
            .db
            .lock()
            .await
            .select_from_where_like("Player", "nickname", &nickname)
            .await;

        assert!(result.len() == 1);

        let (action_send, action_recv) = mpsc::channel(10000);
        let (state_send, state_recv) = mpsc::channel(10000);
        let mut player = Player::new(nickname, state_send, action_recv);

        let row = result.first().unwrap();

        player.id = row.get(0);
        player.coords.x = row.get(2);
        player.coords.y = row.get(3);
        player.coords.z = row.get(4);
        player.direction.x = row.get(5);
        player.direction.y = row.get(6);
        player.direction.z = row.get(7);
        player.current_system = row.get(8);

        let player_id = player.id;
        self.cache.insert(player.id, player);

        (player_id, action_send, state_recv)
    }

    pub(crate) async fn new_player(
        &mut self,
        nickname: String,
    ) -> (&mut Player, Sender<Action>, Receiver<crate::protocol::state::Game>) {
        let (action_send, action_recv) = mpsc::channel(10000);
        let (state_send, state_recv) = mpsc::channel(10000);
        let mut new_player = Player::new(nickname.clone(), state_send, action_recv);
        let id = {
            let mut db = self.db.lock().await;
            if !db
                .select_from_where_like("Player", "nickname", &nickname)
                .await
                .is_empty()
            {
                panic!("Player {} already exists", nickname);
            }
            new_player.id = db
                .insert_row_into(
                    "Player",
                    Some(vec!["nickname".to_string()]),
                    vec![format!("\"{}\"", nickname)],
                    vec![],
                )
                .await;
            // new_player.id = db.last_insert_id().await;
            spacebuild_log!(info, "cache", "last insert id: {}", new_player.id);
            new_player.id
        };
        self.cache.insert(id, new_player);
        let player = self.cache.get_mut(&id).unwrap();
        (player, action_send, state_recv)
    }

    pub(crate) fn new(db: Arc<Mutex<SqlDb>>) -> Self {
        Self {
            cache: HashMap::new(),
            db,
        }
    }

    pub async fn save(&self, id: u32) {
        let player = self.cache.get(&id).unwrap();
        self.db
            .lock()
            .await
            .insert_row_into(
                "Player",
                Some(vec![
                    "id".to_string(),
                    "nickname".to_string(),
                    "coord_x".to_string(),
                    "coord_y".to_string(),
                    "coord_z".to_string(),
                    "direction_x".to_string(),
                    "direction_y".to_string(),
                    "direction_z".to_string(),
                    "current_system".to_string(),
                ]),
                vec![
                    player.id.to_string(),
                    // fixme: sql injection?
                    format!("\"{}\"", player.nickname),
                    player.coords.x.to_string(),
                    player.coords.y.to_string(),
                    player.coords.z.to_string(),
                    player.direction.x.to_string(),
                    player.direction.y.to_string(),
                    player.direction.z.to_string(),
                    player.current_system.to_string(),
                ],
                vec![
                    ("nickname", "nickname"),
                    ("coord_x", "coord_x"),
                    ("coord_y", "coord_y"),
                    ("coord_z", "coord_z"),
                    ("direction_x", "direction_x"),
                    ("direction_y", "direction_y"),
                    ("direction_z", "direction_z"),
                    ("current_system", "current_system"),
                ],
            )
            .await;
    }

    pub(crate) async fn save_all(&self) {
        for (id, _) in &self.cache {
            self.save(*id).await;
        }
    }
}
