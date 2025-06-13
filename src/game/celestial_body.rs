use super::entity::Entity;
use rstar::{RTreeObject, AABB};
use scilib::coordinate::cartesian::Cartesian;

#[derive(Clone, Debug)]
pub struct CelestialBody {
    pub(crate) id: u32,
    pub(crate) owner: u32,
    pub(crate) coords: Cartesian,
    pub(crate) local_direction: Cartesian,
    pub(crate) local_speed: f64,
    pub(crate) angular_speed: f64,
    pub(crate) rotating_speed: f64,
    pub(crate) gravity_center: u32,
    pub(crate) entity: Entity,
}

impl PartialEq for CelestialBody {
    fn eq(&self, other: &Self) -> bool {
        if let Entity::Player(self_player_entity) = &self.entity {
            if let Entity::Player(other_player_entity) = &other.entity {
                self_player_entity.id == other_player_entity.id
            } else {
                false
            }
        } else {
            false
        }
    }
    fn ne(&self, other: &Self) -> bool {
        !(self == other)
    }
}

impl RTreeObject for CelestialBody {
    type Envelope = AABB<[f64; 3]>;

    fn envelope(&self) -> Self::Envelope {
        AABB::from_point([self.coords.x, self.coords.y, self.coords.z])
    }
}

impl CelestialBody {
    pub fn get_uuid(&self) -> u32 {
        self.id
    }

    pub fn get_coords(&self) -> Cartesian {
        self.coords.clone()
    }

    pub fn get_direction(&self) -> Cartesian {
        self.local_direction
    }

    pub fn get_speed(&self) -> f64 {
        self.local_speed
    }

    pub fn borrow_entity(&self) -> &Entity {
        &self.entity
    }

    pub(crate) fn new(
        id: u32,
        owner: u32,
        coords: Cartesian,
        local_direction: Cartesian,
        local_speed: f64,
        angular_speed: f64,
        rotating_speed: f64,
        gravity_center: u32,
        entity: Entity,
    ) -> CelestialBody {
        CelestialBody {
            id,
            owner,
            coords,
            local_speed,
            angular_speed,
            gravity_center,
            rotating_speed,
            local_direction,
            entity,
        }
    }

    // pub(crate) fn dummy(id: Id) -> CelestialBody {
    //     CelestialBody::new(
    //         id,
    //         Id::default(),
    //         Cartesian::default(),
    //         Cartesian::default(),
    //         0f64,
    //         0f64,
    //         0f64,
    //         Id::default(),
    //         Entity::Asteroid(Asteroid { id: Id::default() }),
    //     )
    // }
}
