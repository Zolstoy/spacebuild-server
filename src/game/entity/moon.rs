use crate::Id;

#[derive(Clone, PartialEq, Debug)]
pub struct Moon {
    pub(crate) id: Id,
}

impl Moon {
    pub fn new(id: Id) -> Moon {
        Moon { id }
    }
}
