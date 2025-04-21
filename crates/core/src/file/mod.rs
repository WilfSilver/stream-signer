//! This module stores all tools for interacting with the `srt` format.
//!
//! As a standard, for tests and other purposes, we use the `ssrt` extension to
//! stand for "signing srt"

mod error;
mod signs;
pub mod time;

pub use error::*;
pub use signs::*;
pub use time::Timestamp;
