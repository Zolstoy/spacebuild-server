#[derive(Clone, PartialEq, Debug)]
pub struct Planet {
    pub(crate) id: u32,
}

impl Planet {
    pub fn new(id: u32) -> Planet {
        Planet { id }
    }
}
