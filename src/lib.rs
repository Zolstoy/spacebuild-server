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
mod tests_cache {
    use std::env;

    use uuid::Uuid;

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

    async fn bootstrap(db_path: String, create_erase: bool) -> anyhow::Result<SyncPool> {}

    #[tokio::test]
    async fn case_01_ids() -> anyhow::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests_galaxy {
    use scilib::coordinate::cartesian::Cartesian;

    use crate::game::{
        body::Body,
        entity::{star::Star, Entity},
        galaxy::Galaxy,
    };

    #[test]
    fn case_01() -> anyhow::Result<()> {
        let mut galaxy = Galaxy::default();
        galaxy.insert_celestial(Body::new(
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
