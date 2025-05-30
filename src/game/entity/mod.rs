use asteroid::Asteroid;
use moon::Moon;
use planet::Planet;
use player::Player;
use star::Star;

pub mod asteroid;
pub mod moon;
pub mod planet;
pub mod player;
pub mod star;

#[derive(Clone, PartialEq, Debug)]
pub enum Entity {
    Player(Player),
    Star(Star),
    Asteroid(Asteroid),
    Planet(Planet),
    Moon(Moon),
}
