use crate::Id;

#[derive(Clone, PartialEq, Debug)]
pub struct Star {
    pub(crate) id: Id,
}

impl Star {
    pub fn new(id: Id) -> Star {
        Star { id }
    }
}
