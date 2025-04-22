use std::time::Duration;

#[cfg(feature = "signing")]
use identity_iota::storage::JwkStorageDocumentError;

#[cfg(feature = "verifying")]
use crate::utils::UnknownKey;

use thiserror::Error;

use crate::{file::Timestamp, spec::Vec2u};

use super::pipeline::BuilderError;

pub type StreamError = glib::Error;

/// These are all the different types of errors which could arise while dealing
/// with the library.
///
/// While not actually used here, it can be useful when you are doing everything
/// in one function and just want a catch all.
#[derive(Error, Debug)]
pub enum VideoError {
    #[error(transparent)]
    Builder(#[from] BuilderError),
    #[error(transparent)]
    Stream(#[from] StreamError),
    #[error(transparent)]
    Operaiton(#[from] SigOperationError),
    #[cfg(feature = "signing")]
    #[error(transparent)]
    Sign(#[from] JwkStorageDocumentError),
}

/// Stores all the errors which may arise when handling a frame, including
/// signing and verifying that frame (though it could be the chunk the frame
/// is apart of)
#[derive(Error, Debug, Clone)]
pub enum SigOperationError {
    #[error("Tried to access range which is not covered by the video: {0:.2?} -> {1:.2?}")]
    OutOfRange(Timestamp, Timestamp),
    #[error("The size of the chunk spread over {0:.2?} which is not allowed")]
    InvalidChunkSize(Duration),
    #[error("Could not crop frame with pos {0:?} and size {1:?}")]
    InvalidCrop(Vec2u, Vec2u),
    #[error("The given channels do not exists {0:?}")]
    InvalidChannels(Vec<usize>),
    #[cfg(feature = "verifying")]
    #[error(transparent)]
    UnknownCredential(#[from] UnknownKey),
}
