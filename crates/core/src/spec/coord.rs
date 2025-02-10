use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
pub struct Coord {
    pub x: u32,
    pub y: u32,
}

impl Coord {
    pub const fn new(x: u32, y: u32) -> Self {
        Self { x, y }
    }
}
