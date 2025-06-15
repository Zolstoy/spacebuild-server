use crate::error::Error;
use crate::protocol::{GameState, PlayerAction};
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
    pub async fn get_body(&mut self, id: u32) -> Body {
        let maybe_body = self.bodies.get(&id);
        let body = if maybe_body.is_some() {
            maybe_body.unwrap()
        } else {
            self.add_body(id, self.load_body(id).await)
        };
        body.clone()
    }

    async fn load_body(&self, id: u32) -> Body {
        self.db
            .lock()
            .await
            .select_from_where_equals("Body", "id", id.to_string().as_str())
            .await
            .first()
            .unwrap()
            .into()
    }

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
            self.db.lock().await.insert_rows_into("Body", rows, vec![]);
        }
    }

    pub(crate) fn sync(&mut self, bodies: Vec<&Body>) -> () {
        for body in bodies {
            self.add_body(body.id, body.clone());
        }
    }
}

pub struct PlayerCache {
    pub(crate) players: HashMap<u32, Player>,
    pub db: Arc<Mutex<SqlDb>>,
}

impl PlayerCache {
    pub async fn load_by_nickname(
        &mut self,
        nickname: String,
    ) -> Result<(u32, Sender<PlayerAction>, Receiver<GameState>)> {
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
                game_info_send,
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
                game_info_send,
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
        self.sync(id);
        self.players.remove(&id);
    }
}
