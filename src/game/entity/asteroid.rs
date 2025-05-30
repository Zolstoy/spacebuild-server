use crate::Id;

#[derive(Clone, PartialEq, Debug)]
pub struct Asteroid {
    pub(crate) id: Id,
}

impl Asteroid {
    pub fn new(id: Id) -> Asteroid {
        Asteroid { id }
    }
}
