#[derive(Clone, PartialEq, Debug)]
pub struct Asteroid {
    pub(crate) id: u32,
}

impl Asteroid {
    pub fn new(id: u32) -> Asteroid {
        Asteroid { id }
    }
}
