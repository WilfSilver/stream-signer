//! This is a direct translation of the [specification, found on
//! GitHub](https://github.com/WilfSilver/stream-signer/blob/main/spec.md) with
//! a minimal later on top
//!
//! However note that it does not handle the full file, only each
//! [ChunkSignature], for file abstractions see [crate::file]

mod constants;
mod coord;
mod presentation;
mod signature;

pub use constants::*;
pub use coord::*;
pub use presentation::*;
pub use signature::*;
