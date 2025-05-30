use crate::Id;

use super::{
    entity::{asteroid::Asteroid, Entity},
    repr::Vector3,
};
use rstar::{RTreeObject, AABB};

#[derive(Clone, Debug)]
pub struct CelestialBody {
    pub(crate) id: Id,
    pub(crate) owner: Id,
    pub(crate) coords: Vector3,
    pub(crate) local_direction: Vector3,
    pub(crate) local_speed: f64,
    pub(crate) angular_speed: f64,
    pub(crate) rotating_speed: f64,
    pub(crate) gravity_center: Id,
    pub(crate) entity: Entity,
}

impl PartialEq for CelestialBody {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
    fn ne(&self, other: &Self) -> bool {
        self.id != other.id
    }
}

impl RTreeObject for CelestialBody {
    type Envelope = AABB<[f64; 3]>;

    fn envelope(&self) -> Self::Envelope {
        AABB::from_point([self.coords.x, self.coords.y, self.coords.z])
    }
}

impl CelestialBody {
    pub fn get_uuid(&self) -> Id {
        self.id
    }

    pub fn get_coords(&self) -> Vector3 {
        self.coords.clone()
    }

    pub fn get_direction(&self) -> Vector3 {
        self.local_direction
    }

    pub fn get_speed(&self) -> f64 {
        self.local_speed
    }

    pub fn borrow_entity(&self) -> &Entity {
        &self.entity
    }

    pub(crate) fn new(
        id: Id,
        owner: Id,
        coords: Vector3,
        local_direction: Vector3,
        local_speed: f64,
        angular_speed: f64,
        rotating_speed: f64,
        gravity_center: Id,
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

    pub(crate) fn dummy(id: Id) -> CelestialBody {
        CelestialBody::new(
            id,
            Id::default(),
            Vector3::default(),
            Vector3::default(),
            0f64,
            0f64,
            0f64,
            Id::default(),
            Entity::Asteroid(Asteroid { id: Id::default() }),
        )
    }
}
