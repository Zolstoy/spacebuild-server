use crate::error::Error;
use crate::game::entity::asteroid::Asteroid;
use crate::game::entity::moon::Moon;
use crate::game::entity::planet::Planet;
use crate::game::entity::player::Player;
use crate::game::entity::star::Star;
use crate::game::entity::Entity;
use crate::game::repr::Vector3;
use crate::protocol::GameInfo;
use crate::{game::celestial_body::CelestialBody, sql_database::SqlDatabase};
use crate::{Id, Result};
use itertools::Itertools;
use sqlx::sqlite::SqliteRow;
use sqlx::Row;
use std::collections::HashMap;
use std::{u32, vec};

pub(crate) struct SyncedBody {
    pub(crate) body: CelestialBody,
}

impl SyncedBody {
    pub fn new(celestial: CelestialBody) -> SyncedBody {
        SyncedBody { body: celestial }
    }
}

pub struct SyncPool {
    pub(crate) synced_bodies: HashMap<Id, SyncedBody>,
    pub(crate) database: SqlDatabase,
    pub(crate) body_next_id: Id,
    pub(crate) player_next_id: Id,
}

impl SyncPool {
    pub async fn new(mut database: SqlDatabase) -> Result<SyncPool> {
        let maybe_next_id_in_body = database.max_in("Body", "id").await?;
        let maybe_next_id_in_player = database.max_in("Player", "id").await?;
        let next_id_in_body = if maybe_next_id_in_body.is_none() {
            1
        } else {
            maybe_next_id_in_body.unwrap() + 1
        };
        let next_id_in_player = if maybe_next_id_in_player.is_none() {
            1
        } else {
            maybe_next_id_in_player.unwrap() + 1
        };
        Ok(SyncPool {
            database,
            synced_bodies: HashMap::new(),
            body_next_id: next_id_in_body,
            player_next_id: next_id_in_player,
        })
    }

    pub(crate) fn next_id_in_body(&mut self) -> Id {
        let ret = self.body_next_id;
        self.body_next_id += 1;
        ret
    }

    pub(crate) fn next_id_in_player(&mut self) -> Id {
        let ret = self.player_next_id;
        self.player_next_id += 1;
        ret
    }

    pub fn new_asteroids(&mut self, n: usize) -> Vec<CelestialBody> {
        let mut asteroids = Vec::new();

        for _ in 0..n {
            let id = self.next_id_in_body();
            let body = CelestialBody::new(
                id,
                Id::MAX,
                Vector3::default(),
                Vector3::default(),
                0f64,
                0f64,
                0f64,
                Id::MAX,
                Entity::Asteroid(Asteroid { id: Id::MAX }),
            );
            self.synced_bodies.insert(id, SyncedBody::new(body.clone()));

            asteroids.push(body);
        }

        asteroids
    }

    pub fn new_star(&mut self) -> CelestialBody {
        let celestial = CelestialBody::new(
            self.next_id_in_body(),
            Id::MAX,
            Vector3::default(),
            Vector3::default(),
            0f64,
            0f64,
            0f64,
            Id::MAX,
            Entity::Star(Star { id: Id::MAX }),
        );

        self.synced_bodies
            .insert(celestial.id, SyncedBody::new(celestial.clone()));

        celestial
    }

    pub fn new_planet(&mut self) -> CelestialBody {
        let celestial = CelestialBody::new(
            self.next_id_in_body(),
            Id::MAX,
            Vector3::default(),
            Vector3::default(),
            0f64,
            0f64,
            0f64,
            Id::MAX,
            Entity::Planet(Planet { id: Id::MAX }),
        );

        self.synced_bodies
            .insert(celestial.id, SyncedBody::new(celestial.clone()));

        celestial
    }

    pub fn new_moon(&mut self) -> CelestialBody {
        let celestial = CelestialBody::new(
            self.next_id_in_body(),
            Id::MAX,
            Vector3::default(),
            Vector3::default(),
            0f64,
            0f64,
            0f64,
            Id::MAX,
            Entity::Moon(Moon { id: Id::MAX }),
        );

        self.synced_bodies
            .insert(celestial.id, SyncedBody::new(celestial.clone()));

        celestial
    }

    pub fn new_player(
        &mut self,
        nickname: &str,
        infos_sender: tokio::sync::mpsc::Sender<GameInfo>,
    ) -> CelestialBody {
        let celestial = CelestialBody::new(
            self.next_id_in_body(),
            Id::MAX,
            Vector3::default(),
            Vector3::default(),
            0f64,
            0f64,
            0f64,
            Id::MAX,
            Entity::Player(Player::new(
                self.next_id_in_player(),
                nickname.to_string(),
                infos_sender,
            )),
        );

        self.synced_bodies
            .insert(celestial.id, SyncedBody::new(celestial.clone()));

        celestial
    }

    fn float_from_row(row: &SqliteRow, column_name: &str) -> Result<f64> {
        Ok(row
            .try_get(column_name)
            .map_err(|err| Error::DbLoadError(err))?)
    }

    fn int_from_row(row: &SqliteRow, column_name: &str) -> Result<u32> {
        Ok(row
            .try_get(column_name)
            .map_err(|err| Error::DbLoadError(err))?)
    }

    fn string_from_row(row: &SqliteRow, column_name: &str) -> Result<String> {
        Ok(row
            .try_get(column_name)
            .map_err(|err| Error::DbLoadError(err))?)
    }

    fn id_from_row(row: &SqliteRow, column_name: &str) -> Result<Id> {
        Ok(Self::int_from_row(row, column_name)?)
    }

    fn coordinates_from_row(row: &SqliteRow, column_name_prefix: &str) -> Result<Vector3> {
        Ok(Vector3 {
            x: Self::float_from_row(row, format!("{}_x", column_name_prefix).as_str())?,
            y: Self::float_from_row(row, format!("{}_y", column_name_prefix).as_str())?,
            z: Self::float_from_row(row, format!("{}_z", column_name_prefix).as_str())?,
        })
    }

    fn body_from_row(row: &SqliteRow, entity: Entity, from_join: bool) -> Result<CelestialBody> {
        let id_column_name = if from_join { "body_id" } else { "id" };
        Ok(CelestialBody {
            coords: Self::coordinates_from_row(row, "coordinate")?,
            id: Self::id_from_row(row, id_column_name)?,
            local_direction: Self::coordinates_from_row(row, "local_direction")?,
            local_speed: Self::float_from_row(row, "local_speed")?,
            rotating_speed: Self::float_from_row(row, "rotating_speed")?,
            gravity_center: Self::id_from_row(row, "gravity_center")?,
            entity,
            angular_speed: Self::float_from_row(row, "angular_speed")?,
            owner: Self::id_from_row(row, "owner")?,
        })
    }

    fn player_from_row(
        row: &SqliteRow,
        from_join: bool,
        infos_sender: tokio::sync::mpsc::Sender<GameInfo>,
    ) -> Result<Entity> {
        let id_column_name = if from_join { "player_id" } else { "id" };
        Ok(Entity::Player(Player::new(
            Self::id_from_row(row, id_column_name)?,
            Self::string_from_row(row, "nickname")?,
            infos_sender,
        )))
    }

    fn asteroid_from_row(row: &SqliteRow) -> Result<Entity> {
        Ok(Entity::Asteroid(Asteroid {
            id: Self::id_from_row(row, "id")?,
        }))
    }

    fn planet_from_row(row: &SqliteRow) -> Result<Entity> {
        Ok(Entity::Planet(Planet {
            id: Self::id_from_row(row, "id")?,
        }))
    }

    fn moon_from_row(row: &SqliteRow) -> Result<Entity> {
        Ok(Entity::Moon(Moon {
            id: Self::id_from_row(row, "id")?,
        }))
    }

    fn star_from_row(row: &SqliteRow) -> Result<Entity> {
        Ok(Entity::Star(Star {
            id: Self::id_from_row(row, "id")?,
        }))
    }

    fn value_from_id(id: Id) -> String {
        if id == Id::MAX {
            "NULL".to_string()
        } else {
            format!("{}", id.to_string())
        }
    }

    fn row_from_body(body: &CelestialBody) -> Vec<String> {
        vec![
            Self::value_from_id(body.id),
            Self::value_from_id(body.owner),
            body.coords.x.to_string(),
            body.coords.y.to_string(),
            body.coords.z.to_string(),
            body.local_direction.x.to_string(),
            body.local_direction.y.to_string(),
            body.local_direction.z.to_string(),
            body.local_speed.to_string(),
            body.angular_speed.to_string(),
            body.rotating_speed.to_string(),
            Self::value_from_id(body.gravity_center),
        ]
    }

    fn row_from_player(player_body: &CelestialBody) -> Vec<String> {
        if let Entity::Player(player) = &player_body.entity {
            vec![
                Self::value_from_id(player.id),
                format!("'{}'", player.nickname),
                Self::value_from_id(player_body.id),
            ]
        } else {
            unreachable!()
        }
    }

    fn row_from_star(star_body: &CelestialBody) -> Vec<String> {
        if let Entity::Star(star) = &star_body.entity {
            vec![
                Self::value_from_id(star.id),
                Self::value_from_id(star_body.id),
            ]
        } else {
            unreachable!()
        }
    }

    fn row_from_planet(planet_body: &CelestialBody) -> Vec<String> {
        if let Entity::Planet(planet) = &planet_body.entity {
            vec![
                Self::value_from_id(planet.id),
                Self::value_from_id(planet_body.id),
            ]
        } else {
            unreachable!()
        }
    }

    fn row_from_moon(moon_body: &CelestialBody) -> Vec<String> {
        if let Entity::Moon(moon) = &moon_body.entity {
            vec![
                Self::value_from_id(moon.id),
                Self::value_from_id(moon_body.id),
            ]
        } else {
            unreachable!()
        }
    }

    fn row_from_asteroid(asteroid_body: &CelestialBody) -> Vec<String> {
        if let Entity::Asteroid(asteroid) = &asteroid_body.entity {
            vec![
                Self::value_from_id(asteroid.id),
                Self::value_from_id(asteroid_body.id),
            ]
        } else {
            unreachable!()
        }
    }

    pub async fn get_body(&mut self, id: Id) -> Result<CelestialBody> {
        let maybe_player_body = self.synced_bodies.get(&id);
        if maybe_player_body.is_some() {
            let player_body = maybe_player_body.unwrap();
            return Ok(player_body.body.clone());
        }

        let subtables = vec!["Player", "Asteroid", "Star", "Planet", "Moon"];
        let mut good_results = Vec::new();
        let mut good_table = "";
        for subtable in subtables {
            let results = self
                .database
                .select_from_where_equals(&subtable, "body_id", id.to_string().as_str())
                .await?;
            if results.len() > 0 {
                good_table = subtable;
                good_results = results;
                break;
            }
        }

        if good_results.is_empty() {
            return Err(Error::DbUuidNotFound(id));
        }

        let row = good_results.first().unwrap();

        let entity = if good_table == "Player" {
            // Self::player_from_row(row, false, infos_sender)?
            return Err(Error::UnexpectedTable);
        } else if good_table == "Asteroid" {
            Self::asteroid_from_row(row)?
        } else if good_table == "Star" {
            Self::star_from_row(row)?
        } else if good_table == "Planet" {
            Self::planet_from_row(row)?
        } else if good_table == "Moon" {
            Self::moon_from_row(row)?
        } else {
            todo!()
        };

        let results = self
            .database
            .select_from_where_equals("Body", "id", id.to_string().as_str())
            .await?;

        assert!(results.len() == 1);

        let row = results.first().unwrap();

        let synced_body = SyncedBody::new(Self::body_from_row(row, entity, false)?);
        let id = synced_body.body.id;
        self.synced_bodies.insert(id, synced_body);
        Ok(self.synced_bodies.get(&id).unwrap().body.clone())
    }

    pub async fn get_rotatings(&mut self, id: Id) -> Result<Vec<CelestialBody>> {
        let mut ids = vec![id];
        let mut rotatings = Vec::new();

        loop {
            if ids.is_empty() {
                break;
            }
            let next_id = ids.pop().unwrap();
            let maybe_synced_body = self.synced_bodies.iter().find(|sb| sb.1.body.id == next_id);

            let body = if let Some(synced_body) = maybe_synced_body {
                if let Entity::Player(_) = synced_body.1.body.entity {
                    continue;
                }
                synced_body.1.body.clone()
            } else {
                let body = self.get_body(next_id).await;
                if body.is_err() {
                    continue;
                }
                body.unwrap()
            };

            rotatings.push(body);

            let results = self
                .database
                .select_from_where_equals("Body", "gravity_center", next_id.to_string().as_str())
                .await?;

            for row in results {
                ids.push(row.try_get("id").unwrap());
            }
        }

        Ok(rotatings)
    }

    pub async fn get_player(
        &mut self,
        nickname: &str,
        infos_sender: tokio::sync::mpsc::Sender<GameInfo>,
    ) -> Result<CelestialBody> {
        let maybe_player = self.synced_bodies.iter().find(|sb| {
            if let Entity::Player(player) = &sb.1.body.entity {
                player.nickname == nickname
            } else {
                false
            }
        });

        let player = if maybe_player.is_none() {
            let results = self
                .database
                .select_from_where_equals("Player", "nickname", nickname)
                .await?;

            if results.len() == 0 {
                return Err(Error::DbLoadPlayerByNicknameNotFound);
            }

            assert!(results.len() == 1);

            let player_row = results.first().unwrap();

            let results = self
                .database
                .select_from_where_equals(
                    "Body",
                    "id",
                    player_row.get::<u32, &str>("body_id").to_string().as_str(),
                )
                .await?;

            assert!(results.len() == 1);

            let body_row = results.first().unwrap();

            let player = Self::body_from_row(
                body_row,
                Self::player_from_row(player_row, false, infos_sender)?,
                false,
            )?;

            self.synced_bodies
                .insert(player.id, SyncedBody::new(player.clone()));
            player
        } else {
            let synced_player = maybe_player.unwrap();
            CelestialBody {
                angular_speed: synced_player.1.body.angular_speed,
                coords: synced_player.1.body.coords.clone(),
                gravity_center: synced_player.1.body.gravity_center,
                id: synced_player.1.body.id,
                local_direction: synced_player.1.body.local_direction.clone(),
                local_speed: synced_player.1.body.local_speed,
                owner: synced_player.1.body.owner,
                rotating_speed: synced_player.1.body.rotating_speed,
                entity: Entity::Player(Player::new(
                    synced_player.1.body.id,
                    nickname.to_string(),
                    infos_sender,
                )),
            }
        };

        Ok(player)
    }

    pub fn sync_body(&mut self, body: &CelestialBody) {
        let maybe_synced_body = self.synced_bodies.get_mut(&body.id);
        if let Some(synced_body) = maybe_synced_body {
            synced_body.body = body.clone();
        } else {
            self.synced_bodies
                .insert(body.id, SyncedBody::new(body.clone()));
        }
    }

    pub fn sync(&mut self, bodies: Vec<&CelestialBody>) {
        for body in bodies {
            self.sync_body(body);
        }
    }

    pub async fn get_gravity_centers(&mut self, id: Id) -> Result<Vec<Id>> {
        let results = self
            .database
            .select_from_where_equals("Body", "gravity_center", id.to_string().as_str())
            .await?;

        let mut gravity_centers = Vec::new();
        for row in results {
            gravity_centers.push(row.try_get("id").unwrap());
        }

        Ok(gravity_centers)
    }

    pub(crate) async fn save(&mut self) -> Result<()> {
        let mut body_insert = Vec::default();
        let mut player_insert = Vec::default();
        let mut star_insert = Vec::default();
        let mut planet_insert = Vec::default();
        let mut moon_insert = Vec::default();
        let mut asteroid_insert = Vec::default();

        for synced_body in self
            .synced_bodies
            .values_mut()
            .sorted_by(|sb1, sb2| Ord::cmp(&sb2.body.id, &sb1.body.id))
        {
            body_insert.push(Self::row_from_body(&synced_body.body));
            match &synced_body.body.entity {
                Entity::Asteroid(_) => {
                    asteroid_insert.push(Self::row_from_asteroid(&synced_body.body))
                }
                Entity::Star(_) => star_insert.push(Self::row_from_star(&synced_body.body)),
                Entity::Player(_) => player_insert.push(Self::row_from_player(&synced_body.body)),
                Entity::Planet(_) => planet_insert.push(Self::row_from_planet(&synced_body.body)),
                Entity::Moon(_) => moon_insert.push(Self::row_from_moon(&synced_body.body)),
            }
        }

        if !body_insert.is_empty() {
            self.database
                .insert_rows_into(
                    "Body",
                    body_insert,
                    vec![
                        // ("owner", "owner"),
                        ("coordinate_x", "coordinate_x"),
                        ("coordinate_y", "coordinate_y"),
                        ("coordinate_z", "coordinate_z"),
                        // ("local_direction_x", "local_direction_x"),
                        // ("local_direction_y", "local_direction_y"),
                        // ("local_direction_z", "local_direction_z"),
                        // ("local_speed", "local_speed"),
                        // ("angular_speed", "angular_speed"),
                        // ("rotating_speed", "rotating_speed"),
                        // ("gravity_center", "gravity_center"),
                    ],
                )
                .await?;
        }
        if !player_insert.is_empty() {
            self.database
                .insert_rows_into(
                    "Player",
                    player_insert,
                    vec![("nickname", "nickname"), ("body_id", "body_id")],
                )
                .await?;
        }
        if !star_insert.is_empty() {
            self.database
                .insert_rows_into("Star", star_insert, vec![("body_id", "body_id")])
                .await?;
        }
        if !planet_insert.is_empty() {
            self.database
                .insert_rows_into("Planet", planet_insert, vec![("body_id", "body_id")])
                .await?;
        }
        if !moon_insert.is_empty() {
            self.database
                .insert_rows_into("Moon", moon_insert, vec![("body_id", "body_id")])
                .await?;
        }
        if !asteroid_insert.is_empty() {
            self.database
                .insert_rows_into("Asteroid", asteroid_insert, vec![("body_id", "body_id")])
                .await?;
        }

        Ok(())
    }
}
