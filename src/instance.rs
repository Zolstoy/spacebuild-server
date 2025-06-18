use crate::cache::BodyCache;
use crate::cache::PlayerCache;
use crate::error::Error;
use crate::galaxy::Galaxy;
use crate::protocol::Action;
use crate::spacebuild_log;
use crate::sqldb::SqlDb;
use crate::Result;
use rand::prelude::*;
use rand::random;
use rand::Rng;
use rand_chacha::ChaCha8Rng;
use scilib::coordinate::cartesian::Cartesian;
use scilib::coordinate::spherical::Spherical;
use sqlx::SqlitePool;
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
        self.bodies.save_all().await;
        self.players.save_all().await;
    }

    pub async fn update(&mut self, delta: f64) {
        self.galaxy.update(delta).await;
        self.bodies.sync(self.galaxy.borrow_bodies());
        for (_, player) in &mut self.players.cache {
            let env = Galaxy::galactics_in_spherical_view(&self.galaxy.celestials, player.coords, 10000f64);
            player.update(delta, env).await;
        }
    }

    pub async fn from_path(db_path: &'_ str) -> Result<Instance> {
        if !Path::new(db_path).exists() {
            File::create(db_path).map_err(|err| Error::DbFileCreationError(err))?;
        }

        let pool = SqlitePool::connect(db_path)
            .await
            .map_err(|err| Error::DbOpenError(db_path.to_string(), err))?;

        let db = SqlDb::new(pool);
        let db = Arc::new(Mutex::new(db));

        let mut bodies = BodyCache::new(db.clone());
        bodies.init_db().await;

        let mut players = PlayerCache::new(db.clone());
        players.init_db().await;

        Ok(Instance {
            bodies,
            galaxy: Galaxy::default(),
            players,
            rng: ChaCha8Rng::seed_from_u64(random()),
        })
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

    async fn new_player(&mut self, nickname: String) -> (u32, Sender<Action>, Receiver<crate::protocol::state::Game>) {
        spacebuild_log!(info, "server", "New player, generating spawning bodies...");

        let offset = Spherical::from(
            self.rng.random_range(1000f64..100000f64),
            self.rng.random_range(PI - 0.1..PI + 0.1),
            self.rng.random_range(-TAU..TAU),
        );
        let current_system = self.gen_system(Cartesian::from_coord(offset)).await;

        let _player_offset = Spherical::from(
            self.rng.random_range(200f64..1500f64),
            self.rng.random_range(PI - 0.1..PI + 0.1),
            self.rng.random_range(-TAU..TAU),
        );
        let (player, action_send, state_recv) = self.players.new_player(nickname).await;
        player.coords = Cartesian::from_coord(offset);
        player.current_system = current_system;
        // + Cartesian::from_coord(player_offset)

        // fixme
        // self.galaxy.celestials.insert(player);

        (player.id, action_send, state_recv)
    }

    pub async fn authenticate(
        &mut self,
        nickname: String,
    ) -> Result<(u32, Sender<Action>, Receiver<crate::protocol::state::Game>)> {
        match self.players.can_login(nickname.clone()).await {
            Err(Error::PlayerIsNew) => Ok(self.new_player(nickname).await),
            Ok(_) => Ok(self.login(nickname).await),
            Err(err) => Err(err),
        }
    }

    pub async fn gen_system(&mut self, offset: Cartesian) -> u32 {
        // let phi = self.rng.random_range(-TAU..TAU);
        // let theta = self.rng.random_range(PI - 0.1..PI + 0.1);
        // let distance = self.rng.random_range(10000f64..100000f64);

        let mut star = self.bodies.new_body(1).await.clone();
        star.gravity_center = star.id;
        star.coords = offset;
        star.rotating_speed = 0f64;

        let nb_planets = self.rng.random_range(5..15);
        for _ in 0..nb_planets {
            let mut planet = self.bodies.new_body(2).await.clone();
            planet.rotating_speed = self.rng.random_range(0.0001..0.001);
            let phi = self.rng.random_range(-TAU..TAU);
            let theta = self.rng.random_range(PI - 0.1..PI + 0.1);
            let distance = self.rng.random_range(500f64..4000f64);
            let add_vec = Cartesian::from_coord(Spherical::from(distance, theta, phi));
            planet.coords = star.coords.clone() + add_vec;
            planet.gravity_center = star.id;

            let nb_moons = self.rng.random_range(0..3);

            for _ in 0..nb_moons {
                let mut moon = self.bodies.new_body(3).await.clone();
                moon.rotating_speed = self.rng.random_range(0.005..0.01);
                let phi = self.rng.random_range(-TAU..TAU);
                let theta = self.rng.random_range(PI - 0.1..PI + 0.1);
                let distance = self.rng.random_range(30f64..200f64);
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
            let mut body = self.bodies.get_body(last_id - i as u32).clone();
            body.rotating_speed = self.rng.random_range(0.0001..0.001);
            let phi = self.rng.random_range(-TAU..TAU);
            let theta = self.rng.random_range(PI - 0.1..PI + 0.1);
            let distance = self.rng.random_range(1500f64..4000f64);
            body.coords = star.coords.clone() + Cartesian::from_coord(Spherical::from(distance, theta, phi));
            body.gravity_center = star.id;
            self.galaxy.insert_celestial(body);
        }

        let star_id = star.id;
        self.galaxy.insert_celestial(star);
        star_id
    }

    async fn login(&mut self, nickname: String) -> (u32, Sender<Action>, Receiver<crate::protocol::state::Game>) {
        let (player_id, send, recv) = self.players.load(nickname).await;

        let player = self.players.get_player(player_id);
        let star = self.bodies.load_body(player.current_system).await.clone();
        let gravitings = self.bodies.load_gravitings(star.id).await;
        self.galaxy.insert_celestial(star);
        for graviting in gravitings {
            self.galaxy.insert_celestial(graviting);
        }
        (player_id, send, recv)
    }
}
