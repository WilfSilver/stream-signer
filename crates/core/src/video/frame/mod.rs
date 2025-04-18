//! Contains specific utilities around and interfaces trying to make it easier
//! to interact with a given video frame

mod wrappers;
mod image;
mod rate;
mod state;

pub use wrappers::*;
pub use image::*;
pub use rate::*;
pub use state::*;
