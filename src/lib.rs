#![forbid(unsafe_code)]

pub mod client;
pub mod error;
pub mod game;
pub mod instance;
pub mod network;
pub mod protocol;
pub mod server;
pub mod service;
pub mod sql_database;
pub mod sync_pool;

pub type Result<T> = std::result::Result<T, crate::error::Error>;

pub type Id = u32;

#[cfg(test)]
use test_helpers_async::*;

#[before_all]
#[cfg(test)]
mod tests_sync_pool {
    use std::{env, fs::File};

    use sqlx::SqlitePool;
    use uuid::Uuid;

    use crate::{
        game::entity::Entity, instance::Instance, sql_database::SqlDatabase, sync_pool::SyncPool,
    };

    pub fn before_all() {
        log::info!("Timeout is {}s", TIMEOUT_DURATION);
    }

    const TIMEOUT_DURATION: u64 = 10;

    pub fn get_random_db_path() -> String {
        format!(
            "{}space_build_tests_{}.db",
            env::temp_dir().to_str().unwrap(),
            Uuid::new_v4().to_string()
        )
    }

    async fn bootstrap(db_path: String, create_erase: bool) -> anyhow::Result<SyncPool> {
        if create_erase {
            File::create(db_path.clone())?;
        }

        let pool = SqlitePool::connect(db_path.as_str()).await?;
        let mut database = SqlDatabase { pool };
        Instance::init_db(&mut database).await?;

        Ok(SyncPool::new(database).await?)
    }

    #[tokio::test]
    async fn case_01_ids() -> anyhow::Result<()> {
        let db_path = get_random_db_path();

        let mut sync_pool = bootstrap(db_path, true).await?;

        assert_eq!(1, sync_pool.body_next_id);
        assert_eq!(1, sync_pool.player_next_id);

        let asteroids = sync_pool.new_asteroids(10);

        assert_eq!(10, asteroids.len());

        for i in 1..11 {
            assert_eq!(i, asteroids.iter().nth(i - 1).unwrap().id as usize)
        }

        assert_eq!(11, sync_pool.body_next_id);
        assert_eq!(1, sync_pool.player_next_id);

        let (send, _recv) = tokio::sync::mpsc::channel(1000);
        let player = sync_pool.new_player("test", send);

        assert_eq!(12, sync_pool.body_next_id);
        assert_eq!(2, sync_pool.player_next_id);

        assert_eq!(11, player.id);

        if let Entity::Player(entity) = player.entity {
            assert_eq!(1, entity.id);
        } else {
            unreachable!()
        }

        let star = sync_pool.new_star();

        assert_eq!(13, sync_pool.body_next_id);
        assert_eq!(2, sync_pool.player_next_id);

        assert_eq!(12, star.id);

        if let Entity::Star(entity) = star.entity {
            assert_eq!(u32::MAX, entity.id);
        } else {
            unreachable!()
        }

        Ok(())
    }

    #[tokio::test]
    async fn case_02_save() -> anyhow::Result<()> {
        let db_path = get_random_db_path();

        let mut sync_pool = bootstrap(db_path.clone(), true).await?;

        let _asteroids = sync_pool.new_asteroids(10);
        let (send, _recv) = tokio::sync::mpsc::channel(1000);
        let player = sync_pool.new_player("test", send);
        let _star = sync_pool.new_star();

        sync_pool.save().await?;

        let mut sync_pool = bootstrap(db_path, false).await?;

        let (send, _recv) = tokio::sync::mpsc::channel(1000);
        let player2 = sync_pool.get_player("test", send).await?;

        assert_eq!(player.id, player2.id);

        Ok(())
    }
}
