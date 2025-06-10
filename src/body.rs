use rstar::{RTreeObject, AABB};
use scilib::coordinate::cartesian::Cartesian;
use sqlx::sqlite::SqliteRow;
use sqlx::Row;

#[derive(Clone, Debug, Default)]
pub struct Body {
    pub(crate) id: u32,
    pub(crate) coords: Cartesian,
    pub(crate) rotating_speed: f64,
    pub(crate) gravity_center: u32,
    pub(crate) body_type: u8,
}

impl PartialEq for Body {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
    fn ne(&self, other: &Self) -> bool {
        !(self == other)
    }
}

impl RTreeObject for Body {
    type Envelope = AABB<[f64; 3]>;

    fn envelope(&self) -> Self::Envelope {
        AABB::from_point([self.coords.x, self.coords.y, self.coords.z])
    }
}

impl From<&SqliteRow> for Body {
    fn from(value: &SqliteRow) -> Self {
        Body {
            id: value.get(0),
            body_type: value.get(1),
            coords: Cartesian {
                x: value.get(2),
                y: value.get(3),
                z: value.get(4),
            },
            rotating_speed: value.get(5),
            gravity_center: value.get(6),
        }
    }
}
