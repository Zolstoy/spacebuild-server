use crate::error::Error;
use crate::game::celestial_body::CelestialBody;
use crate::game::entity::Entity;
use crate::game::galaxy::Galaxy;
use crate::game::repr::Vector3;
use crate::protocol::GameInfo;
use crate::sql_database::SqlDatabase;
use crate::sync_pool::SyncPool;
use crate::{Id, Result};
use is_printable::IsPrintable;
use rand::prelude::*;
use rand::Rng;
use rand_chacha::ChaCha8Rng;
use scilib::coordinate::cartesian::Cartesian;
use scilib::coordinate::spherical::Spherical;
use sqlx::SqlitePool;
use std::f64::consts::{PI, TAU};
use std::fs::File;
use std::path::Path;

pub struct Instance {
    pub(crate) sync_pool: SyncPool,
    pub(crate) galaxy: Galaxy,
}

impl Instance {
    pub async fn save_all(&mut self) -> Result<()> {
        self.sync_pool.save().await?;
        Ok(())
    }

    pub async fn update(&mut self, delta: f64) {
        self.galaxy.update(delta).await;
        self.sync_pool.sync(self.galaxy.borrow_bodies());
    }

    pub async fn from_path(db_path: &'_ str) -> Result<Instance> {
        if !Path::new(db_path).exists() {
            File::create(db_path).map_err(|err| Error::DbFileCreationError(err))?;
        }

        let pool = SqlitePool::connect(db_path)
            .await
            .map_err(|err| Error::DbOpenError(db_path.to_string(), err))?;

        let mut db = SqlDatabase { pool };
        Instance::init_db(&mut db).await?;

        Ok(Instance {
            sync_pool: SyncPool::new(db).await?,
            galaxy: Galaxy::default(),
        })
    }

    pub(crate) async fn init_db(db: &mut SqlDatabase) -> Result<()> {
        db.create_table(
            "Body",
            vec![
                // "id INTEGER PRIMARY KEY AUTOINCREMENT",
                "id INTEGER PRIMARY KEY",
                "owner INTEGER",
                "coordinate_x REAL NOT NULL",
                "coordinate_y REAL NOT NULL",
                "coordinate_z REAL NOT NULL",
                "local_direction_x REAL NOT NULL",
                "local_direction_y REAL NOT NULL",
                "local_direction_z REAL NOT NULL",
                "local_speed REAL",
                "angular_speed REAL",
                "rotating_speed REAL",
                "gravity_center INTEGER",
                // "FOREIGN KEY (owner) REFERENCES Player (id) ON DELETE SET NULL",
                // "FOREIGN KEY (gravity_center) REFERENCES Body (id) ON DELETE SET NULL",
            ],
            vec!["id", "owner", "gravity_center"],
        )
        .await?;

        db.create_table(
            "Player",
            vec![
                // "id INTEGER PRIMARY KEY AUTOINCREMENT",
                "id INTEGER PRIMARY KEY",
                "nickname TEXT",
                "body_id INTEGER",
                // "FOREIGN KEY (body_id) REFERENCES Body (id)",
            ],
            vec!["id", "body_id", "nickname"],
        )
        .await?;

        db.create_table(
            "Star",
            vec![
                // "id INTEGER PRIMARY KEY AUTOINCREMENT",
                "id INTEGER PRIMARY KEY",
                "body_id INTEGER",
                // "FOREIGN KEY (body_id) REFERENCES Body (id)",
            ],
            vec!["id", "body_id"],
        )
        .await?;

        db.create_table(
            "Planet",
            vec![
                // "id INTEGER PRIMARY KEY AUTOINCREMENT",
                "id INTEGER PRIMARY KEY",
                "body_id INTEGER",
                // "FOREIGN KEY (body_id) REFERENCES Body (id)",
            ],
            vec!["id", "body_id"],
        )
        .await?;

        db.create_table(
            "Moon",
            vec![
                // "id INTEGER PRIMARY KEY AUTOINCREMENT",
                "id INTEGER PRIMARY KEY",
                "body_id INTEGER",
                // "FOREIGN KEY (body_id) REFERENCES Body (id)",
            ],
            vec!["id", "body_id"],
        )
        .await?;

        db.create_table(
            "Asteroid",
            vec![
                // "id INTEGER PRIMARY KEY AUTOINCREMENT",
                "id INTEGER PRIMARY KEY",
                "body_id INTEGER",
                // "FOREIGN KEY (body_id) REFERENCES Body (id)",
            ],
            vec!["id", "body_id"],
        )
        .await?;

        Ok(())
    }

    pub fn borrow_galaxy(&self) -> &Galaxy {
        &self.galaxy
    }

    pub fn borrow_galaxy_mut(&mut self) -> &mut Galaxy {
        &mut self.galaxy
    }

    pub async fn load_player_by_nickname(
        &mut self,
        nickname: String,
    ) -> Result<(Id, tokio::sync::mpsc::Receiver<GameInfo>)> {
        if nickname.is_empty() || !nickname.is_printable() {
            return Err(Error::InvalidNickname);
        }

        for celestial in &self.galaxy.celestials {
            if let Entity::Player(player) = &celestial.entity {
                if player.nickname == nickname {
                    return Err(Error::PlayerAlreadyAuthenticated);
                }
            }
        }

        let (send, recv) = tokio::sync::mpsc::channel(1000);
        let player = self.sync_pool.get_player(&nickname, send).await?;

        let star = self.sync_pool.get_body(player.gravity_center).await?;
        let rotatings = self.sync_pool.get_rotatings(star.id).await?;

        let id = player.id;

        self.galaxy.celestials.insert(player);

        for rotating in rotatings {
            self.galaxy.celestials.insert(rotating);
        }

        Ok((id, recv))
    }

    pub async fn leave(&mut self, id: Id) -> Result<()> {
        log::info!("Leave for {}", id);
        let maybe_player = self.galaxy.celestials.iter_mut().find(|c| c.id == id);

        if let Some(player) = maybe_player {
            let player = player.clone();
            let maybe_removed = self.galaxy.celestials.remove(&player);

            if let Some(mut removed) = maybe_removed {
                if let Entity::Player(player) = &mut removed.entity {
                    player.actions.clear();
                    self.sync_pool.sync_body(&removed);
                    // self.sync_pool.save_and_unload_player(removed.id).await?;
                } else {
                    unreachable!()
                }
            } else {
                log::error!("COULD NOT REMOVE PLAYER FROM TREE");
                return Err(Error::CriticalCouldNotRemovePlayerFromTree(id));
            }
        } else {
            log::error!(
                "Leave called but player {} not found. Container size is {}",
                id,
                self.galaxy.celestials.iter().count()
            );
            return Err(Error::LeavePlayerNotFound(id));
        }

        Ok(())
    }

    pub async fn authenticate(
        &mut self,
        nickname: &String,
    ) -> Result<(Id, tokio::sync::mpsc::Receiver<GameInfo>)> {
        let maybe_id = self.load_player_by_nickname(nickname.clone()).await;

        match maybe_id {
            Err(Error::DbLoadPlayerByNicknameNotFound) => {
                log::info!("New player, generating spawning bodies...");
                let (star, asteroids) = self.gen_system().await?;
                let player_coords = {
                    let mut rng = ChaCha8Rng::seed_from_u64(0);
                    let phi = rng.random_range(-TAU..TAU);
                    let theta = rng.random_range(PI - 0.1..PI + 0.1);
                    let distance = rng.random_range(1200f64..1750f64);
                    star.coords + Cartesian::from_coord(Spherical::from(distance, theta, phi))
                };

                let star_id = star.id;
                self.galaxy.celestials.insert(star);

                for body in asteroids {
                    self.galaxy.celestials.insert(body);
                }

                let (send, recv) = tokio::sync::mpsc::channel::<GameInfo>(1000);

                let mut player = self.sync_pool.new_player(&nickname, send);
                player.coords = player_coords;
                player.local_speed = 100f64;
                player.gravity_center = star_id;

                let id = player.id;

                self.galaxy.celestials.insert(player);

                Ok((id, recv))
            }
            Ok(id) => Ok(id),
            Err(err) => Err(err),
        }
    }

    pub async fn gen_system(&mut self) -> Result<(CelestialBody, Vec<CelestialBody>)> {
        let mut rng = ChaCha8Rng::seed_from_u64(0);
        let phi = rng.random_range(-TAU..TAU);
        let theta = rng.random_range(PI - 0.1..PI + 0.1);
        let distance = rng.random_range(10000f64..100000f64);
        let coords = Vector3::from_coord(Spherical::from(distance, theta, phi));

        let mut star = self.sync_pool.new_star();

        star.coords = coords.clone();
        star.rotating_speed = 1000f64;

        let mut bodies = Vec::new();

        let nb_planets = rng.random_range(5..15);

        for _ in 0..nb_planets {
            let mut planet = self.sync_pool.new_planet();
            planet.rotating_speed = rng.random_range(0.001..0.01);
            let phi = rng.random_range(-TAU..TAU);
            let theta = rng.random_range(PI - 0.1..PI + 0.1);
            let distance = rng.random_range(500f64..4000f64);
            let add_vec = Vector3::from_coord(Spherical::from(distance, theta, phi));
            let mut cln = coords.clone();
            cln = cln + add_vec;
            planet.coords = cln;
            planet.gravity_center = star.id;

            let nb_moons = rng.random_range(0..3);

            for _ in 0..nb_moons {
                let mut moon = self.sync_pool.new_moon();
                moon.rotating_speed = rng.random_range(0.001..0.01);
                let phi = rng.random_range(-TAU..TAU);
                let theta = rng.random_range(PI - 0.1..PI + 0.1);
                let distance = rng.random_range(100f64..500f64);
                let add_vec = Vector3::from_coord(Spherical::from(distance, theta, phi));
                let mut cln = planet.coords.clone();
                cln = cln + add_vec;
                moon.coords = cln;
                moon.gravity_center = planet.id;
                bodies.push(moon);
            }

            bodies.push(planet);
        }

        let nb_asteroids = rng.random_range(500..2500);

        let mut asteroids = self.sync_pool.new_asteroids(nb_asteroids);

        for asteroid in &mut asteroids {
            asteroid.rotating_speed = rng.random_range(0.001..0.01);
            let phi = rng.random_range(-TAU..TAU);
            let theta = rng.random_range(PI - 0.1..PI + 0.1);
            let distance = rng.random_range(1500f64..4000f64);
            let add_vec = Vector3::from_coord(Spherical::from(distance, theta, phi));

            let mut cln = coords.clone();

            cln = cln + add_vec;

            asteroid.coords = cln;
            asteroid.gravity_center = star.id;
        }

        bodies.append(&mut asteroids);

        Ok((star, bodies))
    }
}
