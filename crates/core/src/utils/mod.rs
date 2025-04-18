//! Simple abstractions to help with key elements of the logic throughout the
//! library, note most of these are behind features, which are activated
//! depending on if they are required by verifying or signing process

#[cfg(feature = "credential_store")]
mod credential;

#[cfg(feature = "delayed_stream")]
mod delayed_stream;

mod time_range;

#[cfg(feature = "credential_store")]
pub use credential::*;

#[cfg(feature = "delayed_stream")]
pub use delayed_stream::*;

pub use time_range::*;
