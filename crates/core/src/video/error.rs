#[cfg(feature = "signing")]
use identity_iota::storage::JwkStorageDocumentError;

#[cfg(feature = "verifying")]
use crate::UnknownKey;

use thiserror::Error;

use crate::file::Timestamp;

use super::BuilderError;

pub type StreamError = glib::Error;

#[derive(Error, Debug)]
pub enum VideoError {
    #[error(transparent)]
    Builder(#[from] BuilderError),
    #[error(transparent)]
    Stream(#[from] StreamError),
    #[error(transparent)]
    Frame(#[from] FrameError),
}

#[derive(Error, Debug)]
pub enum FrameError {
    #[error(transparent)]
    Stream(#[from] StreamError),
    #[error("Tried to access range which is not covered by the video: {0} -> {1}")]
    OutOfRange(Timestamp, Timestamp),
    #[cfg(feature = "signing")]
    #[error(transparent)]
    Sign(#[from] JwkStorageDocumentError),
    #[cfg(feature = "verifying")]
    #[error(transparent)]
    UnknownCredential(#[from] UnknownKey),
}
