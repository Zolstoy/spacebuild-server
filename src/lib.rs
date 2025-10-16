#![forbid(unsafe_code)]

pub mod body;
pub mod bot;
pub mod cache;
pub mod error;
pub mod galaxy;
pub mod http;
pub mod instance;
pub mod player;
pub mod protocol;
pub mod server;
pub mod service;
pub mod sqldb;
pub mod tls;

#[cfg(feature = "tracing")]
pub mod tracing;

pub type Result<T> = std::result::Result<T, crate::error::Error>;

#[macro_export]
macro_rules! spacebuild_log {
    ( $level:ident, $section:expr, $fmt:expr $(, $arg:expr)*) => {
        {
            use colored::Colorize;
            use std::hash::{DefaultHasher, Hash, Hasher};
            let level_str = stringify!($level);
            let color_level = match level_str {
                "trace" => (140, 140, 140),
                "debug" => (150, 172, 100),
                "info" => (240, 240, 240),
                "warn" => (237, 99, 0),
                "error" => (219, 9, 23),
                _ => (20, 20, 20)
            };
            let mut s = DefaultHasher::new();
            $section.to_string().hash(&mut s);
            let hash = s.finish();
            let r = (hash & 0xFF) as u8;
            let g = ((hash & 0xFF00) >> 8) as u8;
            let b = ((hash & 0xFF0000) >> 16) as u8;

            log::$level!("{:>15}|{}", $section.to_string().truecolor(r, g, b),
                format!($fmt, $(
                    $arg,
                )*).truecolor(color_level.0, color_level.1, color_level.2)
            )
        }
    }
}

#[cfg(test)]
use test_helpers_async::*;

#[before_all]
#[cfg(test)]
mod test_01_body_cache {
    use std::{env, fs::File, path::Path, sync::Arc};

    use sqlx::SqlitePool;
    use tokio::sync::Mutex;
    use uuid::Uuid;

    use crate::{body, cache::BodyCache, sqldb::SqlDb};

    pub fn before_all() {
        spacebuild_log!(info, "test", "Timeout is {}s", TIMEOUT_DURATION);
    }

    const TIMEOUT_DURATION: u64 = 10;

    pub fn get_random_db_path() -> String {
        format!(
            "{}space_build_tests_{}.db",
            env::temp_dir().to_str().unwrap(),
            Uuid::new_v4().to_string()
        )
    }

    async fn bootstrap(db_path: &String) -> BodyCache {
        if !Path::new(&db_path).exists() {
            File::create(&db_path).unwrap();
        }
        let pool = SqlitePool::connect(&db_path).await.unwrap();
        let db = SqlDb::new(pool);
        let mut cache = BodyCache::new(Arc::new(Mutex::new(db)));
        cache.init_db().await;
        cache
    }

    #[tokio::test]
    async fn case_01_add_get() -> anyhow::Result<()> {
        let mut cache = bootstrap(&get_random_db_path()).await;
        let mut body = body::Body::default();
        body.id = 0;
        body.body_type = 3;
        body.coords.x = 2f64;
        body.coords.y = 4f64;
        body.coords.z = 6f64;
        body.gravity_center = 0;
        body.rotating_speed = 8f64;
        let body_id = body.id;
        cache.add_body(body_id, body.clone());
        let body_ref = cache.get_body(body_id);
        assert_eq!(body_ref.body_type, body.body_type);
        assert_eq!(body_ref.coords, body.coords);
        assert_eq!(body_ref.gravity_center, body.gravity_center);
        assert_eq!(body_ref.id, body.id);
        assert_eq!(body_ref.rotating_speed, body.rotating_speed);
        Ok(())
    }

    #[tokio::test]
    async fn case_02_add_reload_get() -> anyhow::Result<()> {
        let mut body = body::Body::default();
        body.id = 0;
        body.body_type = 3;
        body.coords.x = 2f64;
        body.coords.y = 4f64;
        body.coords.z = 6f64;
        body.gravity_center = 0;
        body.rotating_speed = 8f64;
        let db_path = get_random_db_path();
        {
            let mut cache = bootstrap(&db_path).await;
            let body_id = body.id;
            cache.add_body(body_id, body.clone());
            cache.save_all().await;
        }
        {
            let mut cache = bootstrap(&db_path).await;
            let body_ref = cache.load_body(body.id).await;
            assert_eq!(body_ref.body_type, body.body_type);
            assert_eq!(body_ref.coords, body.coords);
            assert_eq!(body_ref.gravity_center, body.gravity_center);
            assert_eq!(body_ref.id, body.id);
            assert_eq!(body_ref.rotating_speed, body.rotating_speed);
        }
        Ok(())
    }
}

#[before_all]
#[cfg(test)]
mod test_02_player_cache {
    use std::{env, fs::File, path::Path, sync::Arc};

    use scilib::coordinate::cartesian::Cartesian;
    use sqlx::SqlitePool;
    use tokio::sync::Mutex;
    use uuid::Uuid;

    use crate::{cache::PlayerCache, sqldb::SqlDb, tracing};

    pub fn before_all() {
        spacebuild_log!(info, "test", "Timeout is {}s", TIMEOUT_DURATION);
        tracing::init(Some("(spacebuild.*)".to_string()));
    }

    const TIMEOUT_DURATION: u64 = 10;

    pub fn get_random_db_path() -> String {
        format!(
            "{}space_build_tests_{}.db",
            env::temp_dir().to_str().unwrap(),
            Uuid::new_v4().to_string()
        )
    }

    async fn bootstrap(db_path: &String) -> PlayerCache {
        if !Path::new(&db_path).exists() {
            File::create(&db_path).unwrap();
        }
        let pool = SqlitePool::connect(&db_path).await.unwrap();
        let db = SqlDb::new(pool);
        let mut cache = PlayerCache::new(Arc::new(Mutex::new(db)));
        cache.init_db().await;
        cache
    }

    #[tokio::test]
    async fn case_01_new_player() -> anyhow::Result<()> {
        let mut cache = bootstrap(&get_random_db_path()).await;
        let (player, _, _) = cache.new_player("test123".to_string()).await;
        assert_eq!(1, player.id);
        assert_eq!("test123", player.nickname);
        assert_eq!(Cartesian::default(), player.coords);
        assert_eq!(false, player.first_state_sent);

        Ok(())
    }

    #[tokio::test]
    async fn case_02_new_player_reload() -> anyhow::Result<()> {
        let db_path = get_random_db_path();
        {
            let mut cache = bootstrap(&db_path).await;
            spacebuild_log!(info, "tests", "{}", db_path);
            let (player, _, _) = cache.new_player("test123".to_string()).await;
            assert_eq!(1, player.id);
            assert_eq!("test123", player.nickname);
            assert_eq!(Cartesian::default(), player.coords);
            assert_eq!(false, player.first_state_sent);
        }
        {
            let mut cache = bootstrap(&db_path).await;
            let (id, _, _) = cache.load("test123".to_string()).await;
            assert_eq!(1, id);
        }
        Ok(())
    }

    #[tokio::test]
    async fn case_03_new_player_new_player_diff() -> anyhow::Result<()> {
        let mut cache = bootstrap(&get_random_db_path()).await;
        let (_player1, _, _) = cache.new_player("test123".to_string()).await;
        let (player2, _, _) = cache.new_player("test456".to_string()).await;

        assert_eq!(2, player2.id);
        assert_eq!("test456", player2.nickname);
        assert_eq!(Cartesian::default(), player2.coords);
        assert_eq!(false, player2.first_state_sent);
        Ok(())
    }

    #[tokio::test]
    #[should_panic(expected = "Player test123 already exists")]
    async fn case_04_new_player_new_player_same() {
        let mut cache = bootstrap(&get_random_db_path()).await;
        let (_, _, _) = cache.new_player("test123".to_string()).await;
        let (_, _, _) = cache.new_player("test123".to_string()).await;
    }

    #[tokio::test]
    async fn case_05_two_players_save_reload() -> anyhow::Result<()> {
        let db_path = get_random_db_path();
        {
            let mut cache = bootstrap(&db_path).await;
            let id = {
                let (player, _, _) = cache.new_player("test123".to_string()).await;
                player.coords = Cartesian::from(2, 4, 6);
                player.id
            };
            cache.sync_and_unload(id).await;
            let id = {
                let (player, _, _) = cache.new_player("test456".to_string()).await;
                player.coords = Cartesian::from(3, 5, 7);
                player.id
            };
            cache.sync_and_unload(id).await;
        }
        {
            let mut cache = bootstrap(&db_path).await;
            let (id, _, _) = cache.load("test123".to_string()).await;
            let player = cache.get_player(id);

            assert_eq!(1, player.id);
            assert_eq!("test123", player.nickname);
            assert_eq!(Cartesian::from(2, 4, 6), player.coords);
            assert_eq!(false, player.first_state_sent);

            let (id, _, _) = cache.load("test456".to_string()).await;
            let player = cache.get_player(id);
            assert_eq!(2, player.id);
            assert_eq!("test456", player.nickname);
            assert_eq!(Cartesian::from(3, 5, 7), player.coords);
            assert_eq!(false, player.first_state_sent);
        }
        Ok(())
    }
}

#[before_all]
#[cfg(test)]
mod test_03_player {
    use scilib::coordinate::cartesian::Cartesian;
    use tokio::sync::mpsc::{self};

    use crate::{player::Player, protocol};

    pub fn before_all() {
        spacebuild_log!(info, "test", "Timeout is {}s", TIMEOUT_DURATION);
        // tracing::init(Some("(spacebuild.*)".to_string()));
    }

    const TIMEOUT_DURATION: u64 = 10;

    #[tokio::test]
    async fn case_01_update() -> anyhow::Result<()> {
        let (state_send, _state_recv) = mpsc::channel(10000);
        let (_action_send, action_recv) = mpsc::channel(10000);
        let mut player = Player::new("test123".to_string(), state_send, action_recv);
        player.coords = Cartesian::from(2, 4, 6);
        player.update(1f64, vec![], &Vec::new()).await;
        assert_eq!(Cartesian::from(2, 4, 6), player.coords);
        Ok(())
    }

    #[tokio::test]
    async fn case_02_state_recv() -> anyhow::Result<()> {
        let (state_send, mut state_recv) = mpsc::channel(10000);
        let (_action_send, action_recv) = mpsc::channel(10000);
        let mut player = Player::new("test123".to_string(), state_send, action_recv);
        player.coords = Cartesian::from(2, 4, 6);
        player.update(1f64, vec![], &Vec::new()).await;
        let result = state_recv.try_recv();
        assert!(result.is_ok());
        if let protocol::state::Game::Player(player_state) = result.unwrap() {
            assert_eq!(
                Cartesian::from(2, 4, 6),
                Cartesian::from(player_state.coords[0], player_state.coords[1], player_state.coords[2])
            );
        } else {
            unreachable!();
        }
        Ok(())
    }

    #[tokio::test]
    async fn case_03_action_send_throttle_no_changes() -> anyhow::Result<()> {
        let (state_send, mut state_recv) = mpsc::channel(10000);
        let (action_send, action_recv) = mpsc::channel(10000);
        let mut player = Player::new("test123".to_string(), state_send, action_recv);
        player.coords = Cartesian::from(2, 4, 6);
        action_send
            .send(protocol::Action::ShipState(protocol::ShipState {
                throttle_up: false,
                direction: [0.; 3],
            }))
            .await?;
        player.update(1f64, vec![], &Vec::new()).await;
        let result = state_recv.try_recv();
        assert!(result.is_ok());
        if let protocol::state::Game::Player(player_state) = result.unwrap() {
            assert_eq!(
                Cartesian::from(2, 4, 6),
                Cartesian::from(player_state.coords[0], player_state.coords[1], player_state.coords[2])
            );
        } else {
            unreachable!();
        }
        Ok(())
    }

    #[tokio::test]
    async fn case_04_action_send_throttle_throttle_up_direction_zero() -> anyhow::Result<()> {
        let (state_send, mut state_recv) = mpsc::channel(10000);
        let (action_send, action_recv) = mpsc::channel(10000);
        let mut player = Player::new("test123".to_string(), state_send, action_recv);
        player.coords = Cartesian::from(2, 4, 6);
        action_send
            .send(protocol::Action::ShipState(protocol::ShipState {
                throttle_up: true,
                direction: [0.; 3],
            }))
            .await?;
        player.update(1f64, vec![], &Vec::new()).await;
        let result = state_recv.try_recv();
        assert!(result.is_ok());
        if let protocol::state::Game::Player(player_state) = result.unwrap() {
            assert_eq!(
                Cartesian::from(2, 4, 6),
                Cartesian::from(player_state.coords[0], player_state.coords[1], player_state.coords[2])
            );
        } else {
            unreachable!();
        }
        Ok(())
    }

    #[tokio::test]
    async fn case_05_action_send_throttle_throttle_up_direction_good() -> anyhow::Result<()> {
        let (state_send, mut state_recv) = mpsc::channel(10000);
        let (action_send, action_recv) = mpsc::channel(10000);
        let mut player = Player::new("test123".to_string(), state_send, action_recv);
        player.coords = Cartesian::from(2, 4, 6);
        action_send
            .send(protocol::Action::ShipState(protocol::ShipState {
                throttle_up: true,
                direction: [1f64, 0f64, 0f64],
            }))
            .await?;
        player.update(1f64, vec![], &Vec::new()).await;
        let result = state_recv.try_recv();
        assert!(result.is_ok());
        if let protocol::state::Game::Player(player_state) = result.unwrap() {
            assert!(player_state.coords[0] > 2f64);
            assert_eq!(4f64, player_state.coords[1]);
            assert_eq!(6f64, player_state.coords[2]);
        } else {
            unreachable!();
        }
        Ok(())
    }
}
