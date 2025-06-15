use crate::body::Body;
use crate::cache::BodyCache;
use crate::cache::PlayerCache;
use crate::error::Error;
use crate::galaxy::Galaxy;
use crate::protocol::GameState;
use crate::spacebuild_log;
use crate::sqldb::SqlDb;
use crate::Result;
use rand::prelude::*;
use rand::Rng;
use rand_chacha::ChaCha8Rng;
use scilib::coordinate::cartesian::Cartesian;
use scilib::coordinate::spherical::Spherical;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::f64::consts::{PI, TAU};
use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::mpsc::{self, Receiver};
use tokio::sync::Mutex;

pub struct Instance {
    pub(crate) bodies: BodyCache,
    pub(crate) galaxy: Galaxy,
    pub(crate) players: PlayerCache,
}

impl Instance {
    pub async fn save_all(&mut self) -> () {
        self.bodies.save().await;
    }

    pub async fn update(&mut self, delta: f64) {
        self.galaxy.update(delta).await;
        self.bodies.sync(self.galaxy.borrow_bodies());
    }

    pub async fn from_path(db_path: &'_ str) -> Result<Instance> {
        if !Path::new(db_path).exists() {
            File::create(db_path).map_err(|err| Error::DbFileCreationError(err))?;
        }

        let pool = SqlitePool::connect(db_path)
            .await
            .map_err(|err| Error::DbOpenError(db_path.to_string(), err))?;

        let mut db = SqlDb { pool };
        Instance::init_db(&mut db).await?;

        let db = Arc::new(Mutex::new(db));
        Ok(Instance {
            bodies: BodyCache {
                bodies: HashMap::new(),
                db: db.clone(),
            },
            galaxy: Galaxy::default(),
            players: PlayerCache {
                players: HashMap::new(),
                db,
            },
        })
    }

    pub(crate) async fn init_db(db: &mut SqlDb) -> Result<()> {
        db.create_table(
            "Body",
            vec![
                "id INTEGER PRIMARY KEY AUTOINCREMENT",
                "type INTEGER NOT NULL",
                "coord_x REAL NOT NULL",
                "coord_y REAL NOT NULL",
                "coord_z REAL NOT NULL",
                "rotating_speed REAL",
                "gravity_center INTEGER",
                "FOREIGN KEY (gravity_center) REFERENCES Body (id)",
            ],
            vec!["id", "gravity_center"],
        )
        .await?;

        db.create_table(
            "Player",
            vec![
                "id INTEGER PRIMARY KEY AUTOINCREMENT",
                "nickname TEXT",
                "coord_x REAL NOT NULL",
                "coord_y REAL NOT NULL",
                "coord_z REAL NOT NULL",
                "direction_x REAL",
                "direction_y REAL",
                "direction_z REAL",
            ],
            vec!["id", "nickname"],
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

    pub async fn leave(&mut self, id: u32) {
        spacebuild_log!(info, "instance", "Leave for {}", id);

        self.players.sync_and_unload(id);
    }

    pub async fn authenticate(&mut self, nickname: &String) -> Result<(u32, u32, Receiver<GameState>)> {
        let maybe_id = self.players.load_by_nickname(nickname.clone()).await;

        match maybe_id {
            Err(Error::DbLoadPlayerByNicknameNotFound) => {
                spacebuild_log!(info, "server", "New player, generating spawning bodies...");
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

                let (send, recv) = mpsc::channel(10000);

                let mut player = self.players.new_player(&nickname, send);
                player.coords = player_coords;
                player.local_speed = 100f64;
                player.gravity_center = star_id;

                let body_id = player.id;
                let player_id = if let Entity::Player(player_entity) = &player.entity {
                    player_entity.id
                } else {
                    unreachable!();
                };

                self.galaxy.celestials.insert(player);

                Ok((player_id, body_id, recv))
            }
            Ok(id_recv) => Ok(id_recv),
            Err(err) => Err(err),
        }
    }

    pub async fn gen_system(&mut self) -> Result<(Body, Vec<Body>)> {
        let mut rng = ChaCha8Rng::seed_from_u64(0);
        let phi = rng.random_range(-TAU..TAU);
        let theta = rng.random_range(PI - 0.1..PI + 0.1);
        let distance = rng.random_range(10000f64..100000f64);
        let coords = Cartesian::from_coord(Spherical::from(distance, theta, phi));

        let mut star = self.bodies.new_star();

        star.coords = coords.clone();
        star.rotating_speed = 1000f64;

        let mut bodies = Vec::new();

        let nb_planets = rng.random_range(5..15);

        for _ in 0..nb_planets {
            let mut planet = Body::default();
            planet.rotating_speed = rng.random_range(0.001..0.01);
            let phi = rng.random_range(-TAU..TAU);
            let theta = rng.random_range(PI - 0.1..PI + 0.1);
            let distance = rng.random_range(500f64..4000f64);
            let add_vec = Cartesian::from_coord(Spherical::from(distance, theta, phi));
            let mut cln = coords.clone();
            cln = cln + add_vec;
            planet.coords = cln;
            planet.gravity_center = star.id;

            let nb_moons = rng.random_range(0..3);

            for _ in 0..nb_moons {
                let mut moon = self.bodies.new_moon();
                moon.rotating_speed = rng.random_range(0.001..0.01);
                let phi = rng.random_range(-TAU..TAU);
                let theta = rng.random_range(PI - 0.1..PI + 0.1);
                let distance = rng.random_range(100f64..500f64);
                let add_vec = Cartesian::from_coord(Spherical::from(distance, theta, phi));
                let mut cln = planet.coords.clone();
                cln = cln + add_vec;
                moon.coords = cln;
                moon.gravity_center = planet.id;
                bodies.push(moon);
            }

            bodies.push(planet);
        }

        let nb_asteroids = rng.random_range(500..2500);

        let mut asteroids = self.bodies.new_asteroids(nb_asteroids);

        for asteroid in &mut asteroids {
            asteroid.rotating_speed = rng.random_range(0.001..0.01);
            let phi = rng.random_range(-TAU..TAU);
            let theta = rng.random_range(PI - 0.1..PI + 0.1);
            let distance = rng.random_range(1500f64..4000f64);
            let add_vec = Cartesian::from_coord(Spherical::from(distance, theta, phi));

            let mut cln = coords.clone();

            cln = cln + add_vec;

            asteroid.coords = cln;
            asteroid.gravity_center = star.id;
        }

        bodies.append(&mut asteroids);

        Ok((star, bodies))
    }
}
