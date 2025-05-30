use crate::Id;

#[derive(Clone, PartialEq, Debug)]
pub struct Planet {
    pub(crate) id: Id,
}

impl Planet {
    pub fn new(id: Id) -> Planet {
        Planet { id }
    }
}
