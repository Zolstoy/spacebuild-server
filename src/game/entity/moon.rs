#[derive(Clone, PartialEq, Debug)]
pub struct Moon {
    pub(crate) id: u32,
}

impl Moon {
    pub fn new(id: u32) -> Moon {
        Moon { id }
    }
}
