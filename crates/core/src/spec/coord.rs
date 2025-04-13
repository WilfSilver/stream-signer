use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
pub struct Vec2u {
    pub x: u32,
    pub y: u32,
}

impl Vec2u {
    pub const fn new(x: u32, y: u32) -> Self {
        Self { x, y }
    }
}

impl From<(u32, u32)> for Vec2u {
    fn from((x, y): (u32, u32)) -> Self {
        Self::new(x, y)
    }
}
