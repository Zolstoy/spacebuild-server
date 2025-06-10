#![forbid(unsafe_code)]

pub mod bot;
pub mod error;
pub mod game;
pub mod instance;
pub mod network;
pub mod protocol;
pub mod server;
pub mod service;
pub mod sql_database;
pub mod sync_pool;

#[cfg(feature = "tracing")]
pub mod tracing;

pub type Result<T> = std::result::Result<T, crate::error::Error>;

pub type Id = u32;

#[macro_export]
macro_rules! spacebuild_log {
    ( $level:ident, $section:expr, $fmt:expr $(, $arg:expr)*) => {
        {
            use colored::Colorize;
            use std::hash::{DefaultHasher, Hash, Hasher};
            let level_str = stringify!($level);
            let color_level = match level_str {
                "trace" => (140, 140, 140),
                "debug" => (181, 172, 7),
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
mod tests_sync_pool {
    use std::{env, fs::File};

    use sqlx::SqlitePool;
    use tokio::sync::mpsc::channel;
    use uuid::Uuid;

    use crate::{game::entity::Entity, instance::Instance, sql_database::SqlDatabase, sync_pool::SyncPool};

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

        let (send, _recv) = channel(100);
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
        let (send, _recv) = channel(100);
        let player = sync_pool.new_player("test", send);
        let _star = sync_pool.new_star();

        sync_pool.save().await?;

        let mut sync_pool = bootstrap(db_path, false).await?;

        let (send, _recv) = channel(100);
        let player2 = sync_pool.get_player("test", send).await?;

        assert_eq!(player.id, player2.id);

        Ok(())
    }
}

#[cfg(test)]
mod tests_galaxy {
    use scilib::coordinate::cartesian::Cartesian;

    use crate::game::{
        celestial_body::CelestialBody,
        entity::{star::Star, Entity},
        galaxy::Galaxy,
    };

    #[test]
    fn case_01() -> anyhow::Result<()> {
        let mut galaxy = Galaxy::default();
        galaxy.insert_celestial(CelestialBody::new(
            42,
            0,
            Cartesian::origin(),
            Cartesian::origin(),
            0.,
            0.,
            0.,
            0,
            Entity::Star(Star::new(42)),
        ));

        let celestial_ref = galaxy.borrow_body(42);
        assert!(celestial_ref.is_some());
        assert_eq!(42, celestial_ref.unwrap().id);
        Ok(())
    }
}
