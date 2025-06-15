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
    async fn case_02_add_get_reload() -> anyhow::Result<()> {
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
            cache.save().await;
        }
        {
            let cache = bootstrap(&db_path).await;
            let body_ref = cache._load_body(body.id).await;
            assert_eq!(body_ref.body_type, body.body_type);
            assert_eq!(body_ref.coords, body.coords);
            assert_eq!(body_ref.gravity_center, body.gravity_center);
            assert_eq!(body_ref.id, body.id);
            assert_eq!(body_ref.rotating_speed, body.rotating_speed);
        }
        Ok(())
    }
}
