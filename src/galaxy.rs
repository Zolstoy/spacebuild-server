use super::body::Body;
use core::f64;
use rstar::{RTree, AABB};
use scilib::coordinate::{cartesian::Cartesian, spherical::Spherical};
use std::f64::consts::PI;

#[derive(Default)]
pub struct Galaxy {
    pub(crate) celestials: RTree<Body>,
}

impl Galaxy {
    pub fn insert_celestial(&mut self, celestial: Body) {
        self.celestials.insert(celestial);
    }

    pub fn borrow_bodies(&self) -> Vec<&Body> {
        self.celestials.iter().collect()
    }

    pub fn borrow_body(&self, id: u32) -> Option<&Body> {
        self.celestials.iter().find(|g| g.id == id)
    }

    pub fn borrow_body_mut(&mut self, id: u32) -> Option<&mut Body> {
        self.celestials.iter_mut().find(|g| g.id == id)
    }

    // pub fn _remove_by_id(&mut self, id: Id) -> Option<CelestialBody> {
    //     self.celestials.remove(&CelestialBody::dummy(id))
    // }

    pub fn galactics_in_spherical_view(tree: &RTree<Body>, center: Cartesian, radius: f64) -> Vec<&Body> {
        let radius_sq = radius * radius;
        let min = [center.x - radius, center.y - radius, center.z - radius];
        let max = [center.x + radius, center.y + radius, center.z + radius];
        tree.locate_in_envelope_intersecting(&AABB::from_corners(min, max))
            .filter(|g| {
                let d_sq =
                    (g.coords.x - center.x).powi(2) + (g.coords.y - center.y).powi(2) + (g.coords.z - center.z).powi(2);
                d_sq <= radius_sq
            })
            .collect()
    }

    pub async fn update(&mut self, mut delta: f64) {
        delta *= 10f64;
        if self.celestials.iter().count() < 2 {
            return;
        }

        let mut old_rtree = self.celestials.clone();
        let mut new_rtree = RTree::<Body>::default();
        let mut celestials: Vec<_> = self.celestials.drain().collect();

        while celestials.len() > 0 {
            let mut celestial = celestials.pop().unwrap();
            old_rtree.remove(&celestial);

            let gravity_center = celestials.iter().find(|g| g.id == celestial.gravity_center);

            // if let Entity::Player(player) = &mut celestial.entity {
            //     let env = Self::galactics_in_spherical_view(&old_rtree, celestial.coords, 10000f64);

            //     let (coords, direction, _speed) =
            //         player.update(celestial.coords, celestial.local_speed, delta, env).await;

            //     celestial.coords = coords;
            //     celestial.local_direction = direction;
            // } else
            if let Some(gravity_center) = gravity_center {
                let local_coordinates_car = celestial.coords - gravity_center.coords;
                let local_coordinates_sph = Spherical::from_coord(local_coordinates_car);
                let mut new_coordinates_sph = local_coordinates_sph.clone();
                new_coordinates_sph.phi = new_coordinates_sph.phi + celestial.rotating_speed * delta;

                new_coordinates_sph.phi %= PI;

                let delta_car =
                    Cartesian::from_coord(new_coordinates_sph) - Cartesian::from_coord(local_coordinates_sph);

                if !delta_car.x.is_normal() || !delta_car.y.is_normal() || !delta_car.z.is_normal() {
                    // spacebuild_log!(warn, "galaxy", "NUMBER NOT NORMAL");
                } else {
                    celestial.coords += delta_car;

                    let mut ids = vec![celestial.id];

                    while ids.len() > 0 {
                        let id = ids.pop().unwrap();
                        celestials.iter_mut().for_each(|g| {
                            if g.gravity_center == id {
                                g.coords += delta_car;
                                ids.push(g.id);
                            }
                        });
                    }
                }
            }
            old_rtree.insert(celestial.clone());
            new_rtree.insert(celestial);
        }

        // assert_eq!(old_rtree.iter().count(), new_rtree.iter().count());
        self.celestials = new_rtree;
    }
}
