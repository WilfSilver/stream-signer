//! This stores the core logic for signing and verifying video using
//! GStreamer.

pub mod audio;
mod error;
pub mod frame;
pub mod iter;
pub mod manager;
pub mod pipeline;
#[cfg(feature = "signing")]
pub mod sign;
mod utils;
#[cfg(feature = "verifying")]
pub mod verify;

pub use error::*;
pub use frame::{Frame, FrameState, Framerate};
pub use pipeline::{SignPipeline, wrappers::Pipeline};
#[cfg(feature = "signing")]
pub use sign::{ChunkSigner, Signer, SigningError};
