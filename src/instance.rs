use crate::cache::BodyCache;
use crate::cache::PlayerCache;
use crate::error::Error;
use crate::galaxy::Galaxy;
use crate::protocol::Action;
use crate::protocol::State;
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
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::Sender;
use tokio::sync::Mutex;

pub struct Instance {
    pub(crate) bodies: BodyCache,
    pub(crate) galaxy: Galaxy,
    pub(crate) players: PlayerCache,
    rng: ChaCha8Rng,
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
            rng: ChaCha8Rng::seed_from_u64(0),
        })
    }

    pub(crate) async fn init_db(db: &mut SqlDb) -> Result<()> {
        db.create_table(
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
        .await?;

        db.create_table(
            "Player",
            vec![
                "id INTEGER PRIMARY KEY AUTOINCREMENT",
                "nickname TEXT",
                "coord_x REAL",
                "coord_y REAL",
                "coord_z REAL",
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
        self.players.sync_and_unload(id).await;
    }

    async fn new_player(&mut self, nickname: String) -> (u32, Sender<Action>, Receiver<State>) {
        spacebuild_log!(info, "server", "New player, generating spawning bodies...");

        let offset = Spherical::from(
            self.rng.random_range(1200f64..1750f64),
            self.rng.random_range(PI - 0.1..PI + 0.1),
            self.rng.random_range(-TAU..TAU),
        );
        self.gen_system(Cartesian::from_coord(offset)).await;

        let player_offset = Spherical::from(
            self.rng.random_range(500f64..4000f64),
            self.rng.random_range(PI - 0.1..PI + 0.1),
            self.rng.random_range(-TAU..TAU),
        );
        let (player, action_send, state_recv) = self.players.new_player(nickname).await;
        player.coords = Cartesian::from_coord(offset) + Cartesian::from_coord(player_offset);

        // fixme
        // self.galaxy.celestials.insert(player);

        (player.id, action_send, state_recv)
    }

    pub async fn authenticate(&mut self, nickname: String) -> Result<(u32, Sender<Action>, Receiver<State>)> {
        let data = self.players.load_by_nickname(nickname.clone()).await;

        match data {
            Err(Error::DbLoadPlayerByNicknameNotFound) => Ok(self.new_player(nickname).await),
            Ok(value) => Ok(value),
            Err(err) => Err(err),
        }
    }

    pub async fn gen_system(&mut self, offset: Cartesian) {
        let phi = self.rng.random_range(-TAU..TAU);
        let theta = self.rng.random_range(PI - 0.1..PI + 0.1);
        let distance = self.rng.random_range(10000f64..100000f64);

        let mut star = self.bodies.new_body(1).await.clone();
        star.coords = offset + Cartesian::from_coord(Spherical::from(distance, theta, phi));
        star.rotating_speed = 0f64;

        let nb_planets = self.rng.random_range(5..15);
        for _ in 0..nb_planets {
            let mut planet = self.bodies.new_body(2).await.clone();
            let phi = self.rng.random_range(-TAU..TAU);
            let theta = self.rng.random_range(PI - 0.1..PI + 0.1);
            let distance = self.rng.random_range(500f64..4000f64);
            let add_vec = Cartesian::from_coord(Spherical::from(distance, theta, phi));
            planet.coords = star.coords.clone() + add_vec;
            planet.gravity_center = star.id;

            let nb_moons = self.rng.random_range(0..3);

            for _ in 0..nb_moons {
                let mut moon = self.bodies.new_body(3).await.clone();
                let phi = self.rng.random_range(-TAU..TAU);
                let theta = self.rng.random_range(PI - 0.1..PI + 0.1);
                let distance = self.rng.random_range(100f64..500f64);
                let add_vec = Cartesian::from_coord(Spherical::from(distance, theta, phi));
                let mut cln = planet.coords.clone();
                cln = cln + add_vec;
                moon.coords = cln;
                moon.gravity_center = planet.id;
                self.galaxy.insert_celestial(moon);
            }

            self.galaxy.insert_celestial(planet);
        }

        let nb_asteroids = self.rng.random_range(500..2500);
        let last_id = self.bodies.new_bodies(4, nb_asteroids).await;

        for i in 0..nb_asteroids {
            let mut body = self.bodies.get_body(last_id - i as u32).await.clone();
            body.rotating_speed = self.rng.random_range(0.001..0.01);
            let phi = self.rng.random_range(-TAU..TAU);
            let theta = self.rng.random_range(PI - 0.1..PI + 0.1);
            let distance = self.rng.random_range(1500f64..4000f64);
            body.coords = star.coords.clone() + Cartesian::from_coord(Spherical::from(distance, theta, phi));
            body.gravity_center = star.id;
            self.galaxy.insert_celestial(body);
        }

        self.galaxy.insert_celestial(star);
    }
}
