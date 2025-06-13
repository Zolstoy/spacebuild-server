#[derive(Clone, PartialEq, Debug)]
pub struct Star {
    pub(crate) id: u32,
}

impl Star {
    pub fn new(id: u32) -> Star {
        Star { id }
    }
}
